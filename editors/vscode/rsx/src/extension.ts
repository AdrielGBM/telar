import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  State,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const serverPath = resolveServerPath(context.extensionPath);
  if (!serverPath) {
    vscode.window.showErrorMessage(
      "rsx-analyzer not found. Install it with `cargo install rsx-analyzer` or set rsx.serverPath."
    );
    return;
  }

  const serverOptions: ServerOptions = {
    command: serverPath,
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "rsx" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.rsx"),
    },
  };

  client = new LanguageClient(
    "rsx-analyzer",
    "RSX Analyzer",
    serverOptions,
    clientOptions
  );

  // Hover, go-to-definition and Rust diagnostics for `[logic]`/`[view]` are now served in-process by
  // the rsx-analyzer LSP via its embedded rust-analyzer (T-C1), so there are no client-side bridges:
  // the LSP is the single source of truth, with no duplicate providers or cross-process diagnostics race.
  client.start();
  context.subscriptions.push(client);

  // Status bar item reflecting the LSP connection state (the embedded-analyzer "workspace ready"
  // state is logged to the output channel; surfacing it here is a future refinement).
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  status.text = "$(loading~spin) rsx";
  status.tooltip = "rsx-analyzer";
  status.show();
  context.subscriptions.push(status);
  context.subscriptions.push(
    client.onDidChangeState((e) => {
      if (e.newState === State.Running) status.text = "$(check) rsx";
      else if (e.newState === State.Starting) status.text = "$(loading~spin) rsx";
      else status.text = "$(error) rsx";
    })
  );

  // The "▶ Preview" code lens runs `cargo rsx preview --component <name>` in the file's crate.
  context.subscriptions.push(
    vscode.commands.registerCommand("rsx.preview", (uri?: vscode.Uri | string) => {
      const target =
        typeof uri === "string"
          ? vscode.Uri.parse(uri)
          : (uri ?? vscode.window.activeTextEditor?.document.uri);
      if (!target) return;
      const filePath = target.fsPath;
      const component = path.basename(filePath, ".rsx");
      const cwd = findCrateDir(filePath) ?? path.dirname(filePath);
      const terminal = vscode.window.createTerminal({ name: "rsx preview", cwd });
      terminal.show();
      terminal.sendText(`cargo rsx preview --component ${component}`);
    })
  );
}

/// Nearest ancestor directory containing a `Cargo.toml` (the file's crate), for the preview terminal cwd.
function findCrateDir(file: string): string | undefined {
  let dir = path.dirname(file);
  for (;;) {
    if (fs.existsSync(path.join(dir, "Cargo.toml"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}

// === server discovery ======================================================

function resolveServerPath(extensionPath: string): string | undefined {
  // Prefer the binary bundled inside the VSIX under server/ — this is the precompiled path
  // for platform-specific extension packages that ship rsx-analyzer alongside the extension.
  const bundledName = process.platform === "win32" ? "rsx-analyzer.exe" : "rsx-analyzer";
  const bundled = path.join(extensionPath, "server", bundledName);
  if (fs.existsSync(bundled)) return bundled;

  const config = vscode.workspace.getConfiguration("rsx");
  const configured = config.get<string>("serverPath");
  if (configured && configured.trim().length > 0) {
    return configured.trim();
  }

  const onPath = findOnPath("rsx-analyzer");
  if (onPath) return onPath;

  const cargoBin = path.join(os.homedir(), ".cargo", "bin", "rsx-analyzer");
  if (fs.existsSync(cargoBin)) return cargoBin;

  return undefined;
}

function findOnPath(binary: string): string | undefined {
  const pathEnv = process.env.PATH ?? "";
  const separator = process.platform === "win32" ? ";" : ":";
  const ext = process.platform === "win32" ? ".exe" : "";
  for (const dir of pathEnv.split(separator)) {
    const full = path.join(dir, binary + ext);
    if (fs.existsSync(full)) return full;
  }
  return undefined;
}
