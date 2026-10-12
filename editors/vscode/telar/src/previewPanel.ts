import * as crypto from "crypto";
import * as vscode from "vscode";

import {
  Launches,
  PreviewTarget,
  Workshop,
  servedAt,
  watchSession,
} from "./workshop";

// A first build of the workshop for the browser compiles the whole crate graph to WebAssembly.
const PAGE_BOOT_MS = 20 * 60 * 1000;

type PanelMessage =
  | { type: "show"; href: string; origin: string }
  | { type: "status"; text: string };

export class PreviewPanel implements vscode.Disposable {
  private static current: PreviewPanel | undefined;

  private readonly panel: vscode.WebviewPanel;
  private readonly subscriptions: vscode.Disposable[] = [];
  private wanted: PreviewTarget | undefined;
  private shownId: string | undefined;
  private booting = false;
  private disposed = false;
  private mappedPort: number | undefined;
  private last: PanelMessage | undefined;

  static isOpen(): boolean {
    return PreviewPanel.current !== undefined;
  }

  static async show(
    launches: Launches,
    target: PreviewTarget | undefined,
  ): Promise<void> {
    const panel = (PreviewPanel.current ??= new PreviewPanel(launches));
    panel.panel.reveal(undefined, true);
    if (target) await panel.follow(target);
    else if (!panel.wanted)
      panel.post({
        type: "status",
        text: "Put the cursor in a [preview] of a .rsx file.",
      });
  }

  static async follow(target: PreviewTarget): Promise<void> {
    await PreviewPanel.current?.follow(target);
  }

  private constructor(private readonly launches: Launches) {
    this.panel = vscode.window.createWebviewPanel(
      "telar.preview",
      "Preview",
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      { enableScripts: true, retainContextWhenHidden: true },
    );
    this.subscriptions.push(
      this.panel.onDidDispose(() => this.dispose()),
      this.panel.webview.onDidReceiveMessage((message: unknown) => {
        if (isReady(message) && this.last)
          void this.panel.webview.postMessage(this.last);
      }),
    );
    this.panel.webview.html = page();
  }

  private async follow(target: PreviewTarget): Promise<void> {
    this.wanted = target;
    if (target.id === this.shownId || this.booting) return;
    if (target.workshops.length === 0) {
      this.post({
        type: "status",
        text: "No application in this workspace shows these previews. A library's previews are shown by an application that names it under [telar.previews] include in its telar.toml.",
      });
      return;
    }
    const address = await firstServed(target.workshops);
    if (this.disposed || this.wanted !== target) return;
    if (address) return this.load(target, address);
    const host =
      target.workshops.find((workshop) =>
        this.launches.isStarting(workshop.server),
      ) ?? target.workshops[0];
    const launching = !this.launches.isStarting(host.server);
    const lost = launching ? await watchSession(host.server) : undefined;
    if (this.disposed || this.wanted !== target) return;
    if (launching) {
      this.launches.start(
        host.server,
        "Telar workshop (web)",
        host.packageDir,
        "cargo telar preview --target web",
        PAGE_BOOT_MS,
      );
    }
    this.post({
      type: "status",
      text: "Starting the workshop for the browser. A first build takes a while; its progress is in the Telar workshop (web) terminal.",
    });
    this.booting = true;
    const started = await this.launches.whenUp(
      host.server,
      () => servedAt(host),
      lost,
    );
    this.booting = false;
    if (this.disposed) return;
    if (!started) {
      this.post({
        type: "status",
        text: "The workshop stopped before serving its page. Its terminal says why.",
      });
      return;
    }
    // The cursor may have moved on while the workshop built, to a preview another application shows.
    const latest = this.wanted;
    if (latest) await this.follow(latest);
  }

  private load(target: PreviewTarget, address: URL): void {
    const port = Number(address.port);
    // Maps the port through to the extension host's machine, which is where the server runs when the editor is remote.
    if (port !== this.mappedPort) {
      this.mappedPort = port;
      this.panel.webview.options = {
        enableScripts: true,
        portMapping: [{ webviewPort: port, extensionHostPort: port }],
      };
    }
    const href = new URL(
      `${address.pathname.replace(/\/$/, "")}${target.location}?chrome=0`,
      address,
    );
    this.shownId = target.id;
    this.panel.title = `Preview: ${target.id}`;
    this.post({ type: "show", href: href.href, origin: href.origin });
  }

  private post(message: PanelMessage): void {
    this.last = message;
    void this.panel.webview.postMessage(message);
  }

  dispose(): void {
    this.disposed = true;
    if (PreviewPanel.current === this) PreviewPanel.current = undefined;
    for (const subscription of this.subscriptions) subscription.dispose();
    this.panel.dispose();
  }
}

async function firstServed(workshops: Workshop[]): Promise<URL | undefined> {
  for (const workshop of workshops) {
    const address = await servedAt(workshop);
    if (address) return address;
  }
  return undefined;
}

function isReady(message: unknown): boolean {
  return (
    typeof message === "object" &&
    message !== null &&
    (message as Record<string, unknown>).type === "ready"
  );
}

function page(): string {
  const nonce = crypto.randomBytes(16).toString("base64");
  const policy = [
    "default-src 'none'",
    "frame-src http://localhost:* http://127.0.0.1:*",
    `style-src 'nonce-${nonce}'`,
    `script-src 'nonce-${nonce}'`,
  ].join("; ");
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="${policy}" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style nonce="${nonce}">
      html, body { height: 100%; margin: 0; padding: 0; overflow: hidden; }
      body { background: var(--vscode-editor-background); color: var(--vscode-descriptionForeground); font: var(--vscode-font-size) var(--vscode-font-family); }
      iframe { display: block; width: 100%; height: 100%; border: 0; }
      iframe[hidden], p[hidden] { display: none; }
      p { margin: 0; padding: 16px; line-height: 1.5; }
    </style>
  </head>
  <body>
    <p id="status">Opening the preview…</p>
    <iframe id="canvas" title="Preview" hidden></iframe>
    <script nonce="${nonce}">
      const vscode = acquireVsCodeApi();
      const frame = document.getElementById("canvas");
      const status = document.getElementById("status");
      let origin = null;
      let loaded = false;
      frame.addEventListener("load", () => { loaded = true; });
      addEventListener("message", ({ data }) => {
        if (data.type === "status") {
          status.textContent = data.text;
          status.hidden = false;
          frame.hidden = true;
          return;
        }
        if (data.type !== "show") return;
        status.hidden = true;
        frame.hidden = false;
        if (loaded && origin === data.origin) {
          frame.contentWindow.postMessage({ telarGoto: data.href }, data.origin);
          return;
        }
        origin = data.origin;
        loaded = false;
        frame.src = data.href;
      });
      vscode.postMessage({ type: "ready" });
    </script>
  </body>
</html>`;
}
