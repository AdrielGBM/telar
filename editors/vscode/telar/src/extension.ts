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
      "telar-analyzer not found. Install it with `cargo install telar-analyzer` or set telar.serverPath."
    );
    return;
  }

  const serverOptions: ServerOptions = {
    command: serverPath,
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "rsx" }],
    // Watch `.rs` + `Cargo.toml`/`Cargo.lock` too: the embedded rust-analyzer loads hand-written Rust
    // once, so the server needs didChangeWatchedFiles to refresh them (the LSP never gets didChange for
    // non-`.rsx`). A lockfile change (e.g. `cargo add`) shifts the dependency graph, so it forces a
    // full reload just like `Cargo.toml`. Watched `.rsx` events keep the workspace symbol index fresh.
    synchronize: {
      fileEvents: [
        vscode.workspace.createFileSystemWatcher("**/*.rsx"),
        vscode.workspace.createFileSystemWatcher("**/*.rs"),
        vscode.workspace.createFileSystemWatcher("**/Cargo.toml"),
        vscode.workspace.createFileSystemWatcher("**/Cargo.lock"),
      ],
    },
  };

  client = new LanguageClient(
    "telar-analyzer",
    "telar-analyzer",
    serverOptions,
    clientOptions
  );

  // Status bar item reflecting the LSP connection state. The listener MUST be attached before
  // `client.start()`: for a local stdio server the Starting→Running transition can complete before a
  // post-start listener exists, which left the item stuck on the spinner (never reaching the check).
  // Clicking it reveals the server log. (The embedded-analyzer "workspace ready" state — logged there
  // ~15s after connect — is a future refinement to surface here.)
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  status.text = "$(loading~spin) rsx";
  status.tooltip = "telar-analyzer language server — click to show its log";
  status.command = "telar.showServerLog";
  status.show();
  context.subscriptions.push(status);
  context.subscriptions.push(
    client.onDidChangeState((e) => {
      if (e.newState === State.Running) status.text = "$(check) rsx";
      else if (e.newState === State.Starting) status.text = "$(loading~spin) rsx";
      else status.text = "$(error) rsx";
    })
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("telar.showServerLog", () => client?.outputChannel.show())
  );

  // Hover, go-to-definition and Rust diagnostics for `[logic]`/`[view]` are now served in-process by
  // the telar-analyzer LSP via its embedded rust-analyzer (T-C1), so there are no client-side bridges:
  // the LSP is the single source of truth, with no duplicate providers or cross-process diagnostics race.
  client.start();
  context.subscriptions.push(client);

  // Project the stock rust-analyzer's `cargo check` errors (which land on the generated
  // `.telar/build/*.rs`) back onto the `.rsx`. This reuses the check the user's rust-analyzer already runs
  // on save — no duplicate cargo check — and gives clean, cascade-free semantic errors (wrong fn names,
  // unknown tags, type mismatches) on the source line. Our LSP keeps providing instant syntax errors.
  const cargoDiagnostics = vscode.languages.createDiagnosticCollection("rsx-cargo-check");
  context.subscriptions.push(cargoDiagnostics);
  context.subscriptions.push(
    vscode.languages.onDidChangeDiagnostics((e) => {
      for (const uri of e.uris) {
        if (isGeneratedBuildFile(uri.fsPath)) projectGeneratedDiagnostics(uri, cargoDiagnostics);
      }
    })
  );
  // `cargo check` reads the `.rsx` from disk (the macro re-expands it), and the stock rust-analyzer only
  // re-checks on saves of files *it* owns — never the `.rsx`. So on every `.rsx` save we nudge its
  // flycheck; when it finishes, the projection above re-maps the fresh errors onto the `.rsx`. This
  // reuses the user's existing check (no duplicate `cargo check`). On-save only: on-change would just
  // re-surface the last *saved* state, since the macro never sees the unsaved buffer.
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc.languageId === "rsx" || doc.fileName.endsWith(".rsx")) {
        void vscode.commands.executeCommand("rust-analyzer.runFlycheck").then(undefined, () => {
          // rust-analyzer not installed / no flycheck — projection simply stays at its last state.
        });
      }
    })
  );

  // The "▶ Preview" code lens runs `cargo telar preview --component <name>` in the file's crate.
  context.subscriptions.push(
    vscode.commands.registerCommand("telar.preview", (uri?: vscode.Uri | string) => {
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
      terminal.sendText(`cargo telar preview --component ${component}`);
    })
  );
}

const BUILD_MARKER = `${path.sep}.telar${path.sep}build${path.sep}`;

/// Whether `fsPath` is one of the transpiler's generated build files (`<crate>/.telar/build/<rel>.rs`).
function isGeneratedBuildFile(fsPath: string): boolean {
  return fsPath.endsWith(".rs") && fsPath.includes(BUILD_MARKER);
}

/// Maps a generated `<crate>/.telar/build/<rel>.rs` back to its source `<crate>/src/<rel>.rsx`.
function generatedToSource(genFsPath: string): string | undefined {
  const idx = genFsPath.indexOf(BUILD_MARKER);
  if (idx < 0) return undefined;
  const root = genFsPath.slice(0, idx);
  const rel = genFsPath.slice(idx + BUILD_MARKER.length).replace(/\.rs$/, ".rsx");
  return path.join(root, "src", rel);
}

/// Reads the sibling `.rs.map` (`generated line → Some(.rsx line)`, 0-based). `undefined` if missing.
function readLineMap(genFsPath: string): (number | null)[] | undefined {
  try {
    return JSON.parse(fs.readFileSync(genFsPath + ".map", "utf8"));
  } catch {
    return undefined;
  }
}

/// Re-publishes the `cargo check` (rustc) errors on a generated build file onto its `.rsx` source line.
/// Only `rustc`-sourced errors are taken: rust-analyzer's *in-memory* diagnostics cascade on the
/// tightly-coupled generated view (one bad name → dozens of `E0425`s), whereas `cargo check` reports
/// the root causes cleanly. Mapping is line-granular (the source map is per-line), so the whole `.rsx`
/// line is flagged.
function projectGeneratedDiagnostics(
  genUri: vscode.Uri,
  collection: vscode.DiagnosticCollection
): void {
  const source = generatedToSource(genUri.fsPath);
  if (!source) return;

  // While the `.rsx` has unsaved edits, the `cargo check` errors are still pinned to the last *saved*
  // generated code, but `build_sync` already rewrote the `.rs.map` for the live buffer — re-projecting
  // the stale errors through the now-mismatched map jumps the markers to the wrong line. Freeze instead:
  // keep the existing markers (VS Code shifts them as the user types) and let the next save's flycheck
  // refresh them against a matching map.
  const rsxUri = vscode.Uri.file(source);
  const openDoc = vscode.workspace.textDocuments.find((d) => d.uri.fsPath === rsxUri.fsPath);
  if (openDoc?.isDirty) return;

  const lineMap = readLineMap(genUri.fsPath);
  if (!lineMap) return;

  const mapped: vscode.Diagnostic[] = [];
  for (const d of vscode.languages.getDiagnostics(genUri)) {
    if (d.source !== "rustc" || d.severity !== vscode.DiagnosticSeverity.Error) continue;
    const rsxLine = lineMap[d.range.start.line];
    if (rsxLine === null || rsxLine === undefined) continue;
    const range = new vscode.Range(rsxLine, 0, rsxLine, Number.MAX_SAFE_INTEGER);
    const diag = new vscode.Diagnostic(range, d.message, vscode.DiagnosticSeverity.Error);
    diag.source = "rsx (cargo check)";
    diag.code = d.code;
    mapped.push(diag);
  }
  collection.set(rsxUri, mapped);
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
  // for platform-specific extension packages that ship telar-analyzer alongside the extension.
  const bundledName = process.platform === "win32" ? "telar-analyzer.exe" : "telar-analyzer";
  const bundled = path.join(extensionPath, "server", bundledName);
  if (fs.existsSync(bundled)) return bundled;

  const config = vscode.workspace.getConfiguration("telar");
  const configured = config.get<string>("serverPath");
  if (configured && configured.trim().length > 0) {
    return configured.trim();
  }

  const onPath = findOnPath("telar-analyzer");
  if (onPath) return onPath;

  const cargoBin = path.join(os.homedir(), ".cargo", "bin", "telar-analyzer");
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
