import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import * as vscode from "vscode";
import { exec } from "child_process";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  State,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

const PATCHED_VERSION_KEY = "telar.patchedServerVersion";

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  const serverPath = await resolveServerPath(context);
  if (!serverPath) {
    vscode.window.showErrorMessage(
      "telar-analyzer not found. Build it with `cargo build -p telar-analyzer --release` and point `telar.serverPath` at the binary.",
    );
    return;
  }

  const serverOptions: ServerOptions = {
    command: serverPath,
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "rsx" }],
    // The embedded rust-analyzer loads hand-written Rust once, so the server needs didChangeWatchedFiles to refresh it — the LSP never sends didChange for non-`.rsx` files.
    // A lockfile change (e.g. `cargo add`) shifts the dependency graph, so it forces a full reload just like `Cargo.toml`. Watched `.rsx` events keep the workspace symbol index fresh.
    // The `.rs` globs follow cargo's source layout instead of `**`: every workspace load runs `cargo check`, which writes generated `.rs` under `target/`, and watching those would feed a reload loop.
    synchronize: {
      fileEvents: [
        vscode.workspace.createFileSystemWatcher("**/*.rsx"),
        vscode.workspace.createFileSystemWatcher("**/src/**/*.rs"),
        vscode.workspace.createFileSystemWatcher("**/build.rs"),
        vscode.workspace.createFileSystemWatcher("**/Cargo.toml"),
        vscode.workspace.createFileSystemWatcher("**/Cargo.lock"),
      ],
    },
  };

  client = new LanguageClient(
    "telar-analyzer",
    "telar-analyzer",
    serverOptions,
    clientOptions,
  );

  // Status bar item reflecting the LSP connection state. The listener MUST be attached before
  // `client.start()`: for a local stdio server the Starting→Running transition can complete before a
  // post-start listener exists, which left the item stuck on the spinner (never reaching the check).
  // Clicking it reveals the server log. (The embedded-analyzer "workspace ready" state — logged there
  // ~15s after connect — is a future refinement to surface here.)
  const status = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100,
  );
  status.text = "$(loading~spin) rsx";
  status.tooltip = "telar-analyzer language server — click to show its log";
  status.command = "telar.showServerLog";
  status.show();
  context.subscriptions.push(status);
  context.subscriptions.push(
    client.onDidChangeState((e) => {
      if (e.newState === State.Running) status.text = "$(check) rsx";
      else if (e.newState === State.Starting)
        status.text = "$(loading~spin) rsx";
      else status.text = "$(error) rsx";
    }),
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("telar.showServerLog", () =>
      client?.outputChannel.show(),
    ),
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
  const cargoDiagnostics =
    vscode.languages.createDiagnosticCollection("rsx-cargo-check");
  context.subscriptions.push(cargoDiagnostics);
  context.subscriptions.push(
    vscode.languages.onDidChangeDiagnostics((e) => {
      for (const uri of e.uris) {
        if (isGeneratedBuildFile(uri.fsPath))
          projectGeneratedDiagnostics(uri, cargoDiagnostics);
      }
    }),
  );
  // `cargo check` reads the `.rsx` from disk (the macro re-expands it), and the stock rust-analyzer only
  // re-checks on saves of files *it* owns — never the `.rsx`. So on every `.rsx` save we nudge its
  // flycheck; when it finishes, the projection above re-maps the fresh errors onto the `.rsx`. This
  // reuses the user's existing check (no duplicate `cargo check`). On-save only: on-change would just
  // re-surface the last *saved* state, since the macro never sees the unsaved buffer.
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc.languageId === "rsx" || doc.fileName.endsWith(".rsx")) {
        void vscode.commands
          .executeCommand("rust-analyzer.runFlycheck")
          .then(undefined, () => {
            // rust-analyzer not installed / no flycheck — projection simply stays at its last state.
          });
      }
    }),
  );

  // The "▶ Preview" code lens runs `cargo telar preview --component <name>` in the file's crate.
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "telar.preview",
      (uri?: vscode.Uri | string) => {
        const target =
          typeof uri === "string"
            ? vscode.Uri.parse(uri)
            : (uri ?? vscode.window.activeTextEditor?.document.uri);
        if (!target) return;
        const filePath = target.fsPath;
        const component = path.basename(filePath, ".rsx");
        const cwd = findCrateDir(filePath) ?? path.dirname(filePath);
        const terminal = vscode.window.createTerminal({
          name: "rsx preview",
          cwd,
        });
        terminal.show();
        terminal.sendText(`cargo telar preview --component ${component}`);
      },
    ),
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
  const rel = genFsPath
    .slice(idx + BUILD_MARKER.length)
    .replace(/\.rs$/, ".rsx");
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
  collection: vscode.DiagnosticCollection,
): void {
  const source = generatedToSource(genUri.fsPath);
  if (!source) return;

  // While the `.rsx` has unsaved edits, the `cargo check` errors are still pinned to the last *saved*
  // generated code, but `build_sync` already rewrote the `.rs.map` for the live buffer — re-projecting
  // the stale errors through the now-mismatched map jumps the markers to the wrong line. Freeze instead:
  // keep the existing markers (VS Code shifts them as the user types) and let the next save's flycheck
  // refresh them against a matching map.
  const rsxUri = vscode.Uri.file(source);
  const openDoc = vscode.workspace.textDocuments.find(
    (d) => d.uri.fsPath === rsxUri.fsPath,
  );
  if (openDoc?.isDirty) return;

  const lineMap = readLineMap(genUri.fsPath);
  if (!lineMap) return;

  const mapped: vscode.Diagnostic[] = [];
  for (const d of vscode.languages.getDiagnostics(genUri)) {
    if (d.source !== "rustc" || d.severity !== vscode.DiagnosticSeverity.Error)
      continue;
    const rsxLine = lineMap[d.range.start.line];
    if (rsxLine === null || rsxLine === undefined) continue;
    const range = new vscode.Range(
      rsxLine,
      0,
      rsxLine,
      Number.MAX_SAFE_INTEGER,
    );
    const diag = new vscode.Diagnostic(
      range,
      d.message,
      vscode.DiagnosticSeverity.Error,
    );
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

async function resolveServerPath(
  context: vscode.ExtensionContext,
): Promise<string | undefined> {
  const config = vscode.workspace.getConfiguration("telar");
  const configured = config.get<string>("serverPath");
  if (configured && configured.trim().length > 0) {
    return configured.trim();
  }

  // Checked before PATH so a packaged extension still resolves when the editor starts outside a dev shell.
  const bundledName =
    process.platform === "win32" ? "telar-analyzer.exe" : "telar-analyzer";
  const bundled = path.join(context.extensionPath, "server", bundledName);
  if (fs.existsSync(bundled)) return await prepareBundled(context, bundled);

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

// A platform VSIX ships a binary linked against the generic /lib64/ld-linux-x86-64.so.2, which NixOS answers with a stub loader that refuses to run it. The extension directory may be read-only (it is a store path under nix-vscode-extensions), so relink a private copy, mirroring rust-analyzer's bootstrap.
async function prepareBundled(
  context: vscode.ExtensionContext,
  bundled: string,
): Promise<string> {
  if (process.platform !== "linux" || !(await isNixOs())) return bundled;

  if (!needsRelinking(bundled)) return bundled;

  const storageDir = context.globalStorageUri.fsPath;
  const dest = path.join(storageDir, path.basename(bundled));
  const version = context.extension.packageJSON.version as string;

  if (
    fs.existsSync(dest) &&
    context.globalState.get<string>(PATCHED_VERSION_KEY) === version
  ) {
    return dest;
  }

  try {
    await fs.promises.mkdir(storageDir, { recursive: true });
    await fs.promises.rm(dest, { force: true });
    await fs.promises.copyFile(bundled, dest);
    await patchelf(dest);
    await context.globalState.update(PATCHED_VERSION_KEY, version);
    return dest;
  } catch (err) {
    vscode.window.showWarningMessage(
      `Could not patch the bundled telar-analyzer for NixOS (${err}). Patching builds a derivation, so it needs nix-build and a resolvable <nixpkgs> on NIX_PATH; on a pure-flakes system with no nixpkgs channel entry, set telar.serverPath to a locally built binary instead.`,
    );
    return bundled;
  }
}

async function isNixOs(): Promise<boolean> {
  try {
    const contents = await fs.promises.readFile("/etc/os-release", "utf8");
    const id = contents.split("\n").find((line) => line.startsWith("ID="));
    return (id ?? "").includes("nixos");
  } catch {
    return false;
  }
}

/// Whether `binary` has to be relinked before it can exec here.
// Neither naive check works: `nix-vscode-extensions` unpacks the marketplace VSIX into /nix/store verbatim, so living in the store proves nothing; and NixOS ships a stub at the generic `/lib64/ld-linux-*` whose whole job is to refuse these binaries, so the interpreter existing proves nothing either. Only a loader inside the store does.
function needsRelinking(binary: string): boolean {
  const interpreter = elfInterpreter(binary);
  if (interpreter === undefined) return false;
  return !(interpreter.startsWith("/nix/store/") && fs.existsSync(interpreter));
}

const PT_INTERP = 3;

/// Reads an ELF's interpreter (`PT_INTERP`) from its program headers, so asking needs no toolchain. `undefined` when the file is not an ELF, is statically linked, or is too malformed to read — none of which are patchable anyway.
function elfInterpreter(file: string): string | undefined {
  let fd: number | undefined;
  try {
    fd = fs.openSync(file, "r");
    const header = Buffer.alloc(64);
    if (fs.readSync(fd, header, 0, header.length, 0) < header.length)
      return undefined;
    if (header.readUInt32BE(0) !== 0x7f454c46) return undefined;

    const wide = header[4] === 2;
    const little = header[5] === 1;
    const read = (buf: Buffer, offset: number, bytes: number): number => {
      if (bytes === 8)
        return Number(
          little ? buf.readBigUInt64LE(offset) : buf.readBigUInt64BE(offset),
        );
      if (bytes === 4)
        return little ? buf.readUInt32LE(offset) : buf.readUInt32BE(offset);
      return little ? buf.readUInt16LE(offset) : buf.readUInt16BE(offset);
    };

    const phoff = wide ? read(header, 0x20, 8) : read(header, 0x1c, 4);
    const phentsize = wide ? read(header, 0x36, 2) : read(header, 0x2a, 2);
    const phnum = wide ? read(header, 0x38, 2) : read(header, 0x2c, 2);
    if (phentsize < (wide ? 0x38 : 0x20)) return undefined;

    const entry = Buffer.alloc(phentsize);
    for (let i = 0; i < phnum; i++) {
      if (
        fs.readSync(fd, entry, 0, phentsize, phoff + i * phentsize) < phentsize
      )
        return undefined;
      if (read(entry, 0, 4) !== PT_INTERP) continue;

      const offset = wide ? read(entry, 0x08, 8) : read(entry, 0x04, 4);
      const size = wide ? read(entry, 0x20, 8) : read(entry, 0x10, 4);
      if (size === 0 || size > 4096) return undefined;
      const interpreter = Buffer.alloc(size);
      if (fs.readSync(fd, interpreter, 0, size, offset) < size)
        return undefined;
      return interpreter.toString("utf8").replace(/\0.*$/s, "");
    }
    return undefined;
  } catch {
    return undefined;
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

async function patchelf(dest: string): Promise<void> {
  const expression = `
    {srcStr, pkgs ? import <nixpkgs> {}}:
      pkgs.stdenv.mkDerivation {
        name = "telar-analyzer";
        src = /. + srcStr;
        phases = [ "installPhase" "fixupPhase" ];
        installPhase = "cp $src $out";
        fixupPhase = ''
          chmod 755 $out
          patchelf --set-interpreter "$(cat $NIX_CC/nix-support/dynamic-linker)" $out
        '';
      }
  `;
  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Patching telar-analyzer for NixOS",
    },
    async () => {
      // `nix-build -o` replaces dest with a symlink into the store, so the unpatched copy has to move aside first.
      const orig = `${dest}-orig`;
      await fs.promises.rename(dest, orig);
      try {
        await new Promise<void>((resolve, reject) => {
          const handle = exec(
            `nix-build -E - --argstr srcStr '${orig}' -o '${dest}'`,
            (err, _stdout, stderr) =>
              err ? reject(new Error(stderr)) : resolve(),
          );
          handle.stdin?.write(expression);
          handle.stdin?.end();
        });
      } finally {
        await fs.promises.rm(orig, { force: true });
      }
    },
  );
}
