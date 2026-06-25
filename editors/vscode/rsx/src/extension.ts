import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let logicDiagnostics: vscode.DiagnosticCollection | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const serverPath = resolveServerPath();
  if (!serverPath) {
    vscode.window.showErrorMessage(
      "rsx-analyzer not found. Install it with `cargo install cargo-rsx` or set rsx.serverPath."
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

  client.start();
  context.subscriptions.push(client);

  activateLogicDiagnosticsBridge(context);
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}

// === [logic] Rust diagnostics bridge =======================================
//
// rust-analyzer already analyzes the transpiler's generated `.rsx/build/*.rs` files (they are
// `include!`-ed into the app), so the Rust errors in a `[logic]` zone already exist — on the wrong
// file. This mirrors Volar's approach at the client level: read rust-analyzer's diagnostics on the
// build file and republish them onto the originating `.rsx`, mapping line numbers through the source
// map the transpiler writes next to each build file (`<build>.rs.map`).

function activateLogicDiagnosticsBridge(context: vscode.ExtensionContext): void {
  logicDiagnostics = vscode.languages.createDiagnosticCollection("rsx-logic");
  context.subscriptions.push(logicDiagnostics);

  context.subscriptions.push(
    vscode.languages.onDidChangeDiagnostics((event) => {
      for (const uri of event.uris) {
        if (isBuildFile(uri)) {
          bridgeBuildFile(uri);
        }
      }
    })
  );

  // Saving a `.rsx` does not trigger rust-analyzer's `cargo check`, so its build-file diagnostics go
  // stale. Nudge a flycheck on save so the generated Rust is re-checked and the bridge refreshes.
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc.languageId === "rsx" || doc.fileName.endsWith(".rsx")) {
        vscode.commands
          .executeCommand("rust-analyzer.runFlycheck", null)
          .then(undefined, () => {
            /* rust-analyzer extension not present — nothing to refresh. */
          });
      }
    })
  );

  // Bridge anything rust-analyzer has already reported before this extension activated.
  for (const [uri] of vscode.languages.getDiagnostics()) {
    if (isBuildFile(uri)) {
      bridgeBuildFile(uri);
    }
  }
}

const BUILD_MARKER = `${path.sep}.rsx${path.sep}build${path.sep}`;

function isBuildFile(uri: vscode.Uri): boolean {
  return (
    uri.scheme === "file" &&
    uri.fsPath.includes(BUILD_MARKER) &&
    uri.fsPath.endsWith(".rs")
  );
}

/// Maps `<root>/.rsx/build/<rel>.rs` back to its source `<root>/src/<rel>.rsx`.
function rsxSourceFor(buildPath: string): string | undefined {
  const idx = buildPath.indexOf(BUILD_MARKER);
  if (idx < 0) return undefined;
  const root = buildPath.slice(0, idx);
  const rel = buildPath.slice(idx + BUILD_MARKER.length).replace(/\.rs$/, ".rsx");
  return path.join(root, "src", rel);
}

function loadSourceMap(buildPath: string): (number | null)[] | undefined {
  try {
    return JSON.parse(fs.readFileSync(buildPath + ".map", "utf8"));
  } catch {
    return undefined;
  }
}

function bridgeBuildFile(buildUri: vscode.Uri): void {
  if (!logicDiagnostics) return;
  const sourcePath = rsxSourceFor(buildUri.fsPath);
  if (!sourcePath) return;
  const sourceUri = vscode.Uri.file(sourcePath);

  const sourceMap = loadSourceMap(buildUri.fsPath);
  if (!sourceMap) {
    // No map (project built with an older toolchain) — nothing to remap.
    logicDiagnostics.delete(sourceUri);
    return;
  }

  const mapped: vscode.Diagnostic[] = [];
  for (const diag of vscode.languages.getDiagnostics(buildUri)) {
    const rsxLine = sourceMap[diag.range.start.line];
    if (rsxLine === null || rsxLine === undefined) continue; // boilerplate / injected line
    const range = new vscode.Range(rsxLine, 0, rsxLine, Number.MAX_SAFE_INTEGER);
    const bridged = new vscode.Diagnostic(range, diag.message, diag.severity);
    bridged.source = "rsx (rust)";
    bridged.code = diag.code;
    mapped.push(bridged);
  }
  logicDiagnostics.set(sourceUri, mapped);
}

// === server discovery ======================================================

function resolveServerPath(): string | undefined {
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
