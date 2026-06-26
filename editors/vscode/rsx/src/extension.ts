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
  activateLogicIntellisenseBridge(context);
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

// === [logic] Rust IntelliSense bridge ======================================
//
// The reverse of the diagnostics bridge: rust-analyzer already provides completion / hover /
// definition on the generated `.rsx/build/*.rs` (it is `include!`-ed into the real crate, so the
// full type context exists there). We map the `.rsx` cursor *into* the generated file, delegate to
// rust-analyzer via `vscode.execute*Provider`, then map results back. Scoped to the `[logic]` zone,
// where a source line is emitted verbatim with a fixed indent so the mapping is near-exact; `[view]`
// is structurally rewritten and stays with the rsx-analyzer LSP's own (.rsx-domain) providers.

// `app!` emits each `[logic]` line verbatim under a 4-space function-body indent, so a `.rsx` column
// maps to the generated column by adding this. Lines rewritten by the `move`-closure clone pass can
// skew after the first rewritten signal — a known v1 limitation (slightly-off position, never wrong file).
const LOGIC_INDENT = 4;

function activateLogicIntellisenseBridge(context: vscode.ExtensionContext): void {
  const selector: vscode.DocumentSelector = { scheme: "file", language: "rsx" };

  // Hover and go-to-definition only: both query already-built symbols, so they tolerate the small lag
  // between the LSP rewriting the generated `.rs` and rust-analyzer re-reading it. Completion is not
  // bridged — it needs the in-flight text in rust-analyzer at the exact instant the `.` is typed, a
  // cross-process race the client cannot win; reliable completion needs the LSP to drive its own
  // rust-analyzer over the live document (future work).
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(selector, {
      provideHover: provideLogicHover,
    }),
    vscode.languages.registerDefinitionProvider(selector, {
      provideDefinition: provideLogicDefinition,
    })
  );
}

/// Which `.rsx` section a line falls in, mirroring `rsx-analyzer`'s `position::find_section_at`.
function sectionAt(document: vscode.TextDocument, line: number): string {
  let section = "unknown";
  const last = Math.min(line, document.lineCount - 1);
  for (let i = 0; i <= last; i++) {
    switch (document.lineAt(i).text.trim()) {
      case "[logic]":
        section = "logic";
        break;
      case "[props]":
        section = "props";
        break;
      case "[style]":
        section = "style";
        break;
      case "[view]":
        section = "view";
        break;
    }
  }
  return section;
}

/// Inverse of `app!`'s output mirroring: `<crate>/src/<rel>.rsx` -> `<crate>/.rsx/build/<rel>.rs`,
/// where `<crate>` is the nearest ancestor holding a `Cargo.toml` (the macro's `CARGO_MANIFEST_DIR`).
function buildFileFor(rsxPath: string): string | undefined {
  let dir = path.dirname(rsxPath);
  let crateRoot: string | undefined;
  for (;;) {
    if (fs.existsSync(path.join(dir, "Cargo.toml"))) {
      crateRoot = dir;
      break;
    }
    const parent = path.dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
  const rel = path.relative(path.join(crateRoot, "src"), rsxPath);
  if (rel.startsWith("..") || path.isAbsolute(rel)) return undefined;
  return path.join(crateRoot, ".rsx", "build", rel.replace(/\.rsx$/, ".rs"));
}

interface GenTarget {
  uri: vscode.Uri;
  position: vscode.Position;
}

/// Translates an `.rsx` cursor in the `[logic]` zone to its position in the generated `.rs`, or
/// `undefined` when out of zone, unbuilt, or unmapped (so the caller falls through to other providers).
function toGenTarget(
  document: vscode.TextDocument,
  position: vscode.Position
): GenTarget | undefined {
  if (sectionAt(document, position.line) !== "logic") return undefined;
  const buildPath = buildFileFor(document.uri.fsPath);
  if (!buildPath || !fs.existsSync(buildPath)) return undefined;
  const map = loadSourceMap(buildPath);
  if (!map) return undefined;
  // First generated line that originated from this `.rsx` line; for `[logic]` this is its verbatim Rust line.
  const genLine = map.indexOf(position.line);
  if (genLine < 0) return undefined;
  return {
    uri: vscode.Uri.file(buildPath),
    position: new vscode.Position(genLine, position.character + LOGIC_INDENT),
  };
}

async function provideLogicHover(
  document: vscode.TextDocument,
  position: vscode.Position
): Promise<vscode.Hover | undefined> {
  const target = toGenTarget(document, position);
  if (!target) return undefined;
  const hovers = await vscode.commands.executeCommand<vscode.Hover[]>(
    "vscode.executeHoverProvider",
    target.uri,
    target.position
  );
  const hover = hovers?.find((h) => h.contents.length > 0);
  if (!hover) return undefined;
  // Drop the generated-coordinate range; let VSCode highlight the hovered `.rsx` word itself.
  return new vscode.Hover(hover.contents);
}

async function provideLogicDefinition(
  document: vscode.TextDocument,
  position: vscode.Position
): Promise<vscode.Location[] | undefined> {
  const target = toGenTarget(document, position);
  if (!target) return undefined;
  const results = await vscode.commands.executeCommand<
    (vscode.Location | vscode.LocationLink)[]
  >("vscode.executeDefinitionProvider", target.uri, target.position);
  if (!results) return undefined;

  const out: vscode.Location[] = [];
  for (const result of results) {
    const isLink = "targetUri" in result;
    const uri = isLink ? result.targetUri : result.uri;
    const range = isLink ? result.targetRange : result.range;
    if (isBuildFile(uri)) {
      // A definition that lands in a generated file (this or another component) → jump to its `.rsx`.
      const mapped = mapBuildLineToRsx(uri, range.start.line);
      if (mapped) out.push(mapped);
    } else {
      // Real library/app source → follow into the actual Rust unchanged.
      out.push(new vscode.Location(uri, range));
    }
  }
  return out;
}

/// Maps a location on a generated `.rs` line back to its `.rsx` source via that file's `<build>.rs.map`.
function mapBuildLineToRsx(
  buildUri: vscode.Uri,
  genLine: number
): vscode.Location | undefined {
  const sourcePath = rsxSourceFor(buildUri.fsPath);
  if (!sourcePath) return undefined;
  const map = loadSourceMap(buildUri.fsPath);
  const rsxLine = map?.[genLine];
  if (rsxLine === null || rsxLine === undefined) return undefined;
  return new vscode.Location(
    vscode.Uri.file(sourcePath),
    new vscode.Position(rsxLine, 0)
  );
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
