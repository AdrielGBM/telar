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
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}

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
