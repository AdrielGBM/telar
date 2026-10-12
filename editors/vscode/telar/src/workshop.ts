import * as fs from "fs";
import * as http from "http";
import * as net from "net";
import * as vscode from "vscode";

export interface Workshop {
  packageDir: string;
  channel: string;
  server: string;
}

export interface PreviewTarget {
  id: string;
  packageDir: string;
  location: string;
  workshops: Workshop[];
}

// A preview id is built from a crate name and slugs alone, so anything else is refused rather than handed to a shell.
const PREVIEW_ID = /^[\p{L}\p{N}_-]+$/u;

export function isWorkshop(value: unknown): value is Workshop {
  if (typeof value !== "object" || value === null) return false;
  const workshop = value as Record<string, unknown>;
  return (
    typeof workshop.packageDir === "string" &&
    typeof workshop.channel === "string" &&
    typeof workshop.server === "string"
  );
}

export function isWorkshopList(value: unknown): value is Workshop[] {
  return Array.isArray(value) && value.every(isWorkshop);
}

export function isPreviewTarget(value: unknown): value is PreviewTarget {
  if (typeof value !== "object" || value === null) return false;
  const target = value as Record<string, unknown>;
  return (
    typeof target.id === "string" &&
    PREVIEW_ID.test(target.id) &&
    typeof target.packageDir === "string" &&
    typeof target.location === "string" &&
    target.location.startsWith("/") &&
    isWorkshopList(target.workshops)
  );
}

// The wire format of cargo-telar's `runner/workshop_channel.rs`: one `goto:<location>` or `focus` line, answered `ok` once the session has taken it.
const GOTO_PREFIX = "goto:";
const FOCUS_LINE = "focus";
const ACCEPTED = "ok";
const ANSWER_TIMEOUT_MS = 2000;

export function goto(workshop: Workshop, location: string): Promise<boolean> {
  return request(workshop.channel, `${GOTO_PREFIX}${location}`);
}

export function focus(workshop: Workshop): Promise<boolean> {
  return request(workshop.channel, FOCUS_LINE);
}

async function request(channel: string, line: string): Promise<boolean> {
  let port: number;
  try {
    port = Number((await fs.promises.readFile(channel, "utf8")).trim());
  } catch {
    return false;
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) return false;
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: "127.0.0.1", port });
    let answer = "";
    const settle = (accepted: boolean) => {
      socket.destroy();
      resolve(accepted);
    };
    socket.setEncoding("utf8");
    socket.setTimeout(ANSWER_TIMEOUT_MS, () => settle(false));
    socket.on("connect", () => socket.write(`${line}\n`));
    socket.on("data", (chunk: string) => {
      answer += chunk;
      const end = answer.indexOf("\n");
      if (end >= 0) settle(answer.slice(0, end).trim() === ACCEPTED);
    });
    socket.on("end", () => settle(answer.trim() === ACCEPTED));
    socket.on("error", () => settle(false));
  });
}

// The wire format of `telar_project::protocol::parse_server_file`: `starting <pid>` while the session builds, the address once it serves.
const SERVER_STARTING_PREFIX = "starting ";

type ServerFile =
  { kind: "starting"; pid: number } | { kind: "serving"; text: string };

async function readServerFile(file: string): Promise<string | undefined> {
  try {
    return (await fs.promises.readFile(file, "utf8")).trim() || undefined;
  } catch {
    return undefined;
  }
}

function parseServerFile(text: string): ServerFile | undefined {
  if (!text.startsWith(SERVER_STARTING_PREFIX)) {
    return { kind: "serving", text };
  }
  const pid = Number(text.slice(SERVER_STARTING_PREFIX.length));
  return Number.isInteger(pid) && pid > 0
    ? { kind: "starting", pid }
    : undefined;
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}

// Follows the session launched for `file` from now on, and answers whether it is gone: it announced itself as starting under a process that has died, or its file appeared and was removed. What the file held when watching began is a leftover of an earlier session and says nothing about this one.
export async function watchSession(
  file: string,
): Promise<() => Promise<boolean>> {
  const leftover = await readServerFile(file);
  let announced = false;
  return async () => {
    const text = await readServerFile(file);
    if (text === leftover) return false;
    if (text === undefined) return announced;
    announced = true;
    const state = parseServerFile(text);
    return state?.kind === "starting" && !isAlive(state.pid);
  };
}

export async function servedAt(workshop: Workshop): Promise<URL | undefined> {
  const text = await readServerFile(workshop.server);
  const state = text === undefined ? undefined : parseServerFile(text);
  if (state?.kind !== "serving") return undefined;
  let address: URL;
  try {
    address = new URL(state.text);
  } catch {
    return undefined;
  }
  // Only this machine's loopback: the panel's content security policy frames nothing else.
  const local = ["localhost", "127.0.0.1"].includes(address.hostname);
  if (address.protocol !== "http:" || !local || !address.port) return undefined;
  return (await answers(address)) ? address : undefined;
}

// Every page served for development polls this build counter, so a server that answers it is one `cargo telar` runs.
function answers(address: URL): Promise<boolean> {
  return new Promise((resolve) => {
    const probe = http.get(
      new URL("/telar-build", address),
      { timeout: ANSWER_TIMEOUT_MS },
      (response) => {
        response.resume();
        resolve(response.statusCode === 200);
      },
    );
    probe.on("timeout", () => probe.destroy());
    probe.on("error", () => resolve(false));
  });
}

interface Launch {
  terminal: vscode.Terminal;
  until: number;
}

// Keyed by the file each session writes once it is up, so a request made while one boots waits for it instead of starting a second.
export class Launches implements vscode.Disposable {
  private readonly starting = new Map<string, Launch>();
  private readonly subscriptions: vscode.Disposable[] = [];

  constructor() {
    this.subscriptions.push(
      vscode.window.onDidCloseTerminal((terminal) =>
        this.forgetTerminal(terminal),
      ),
    );
    // Shell integration says when the command ends, which a session that failed to build does long before its terminal closes; older editors have no such event.
    const ended = vscode.window.onDidEndTerminalShellExecution as
      typeof vscode.window.onDidEndTerminalShellExecution | undefined;
    if (ended) {
      this.subscriptions.push(
        ended((event) => this.forgetTerminal(event.terminal)),
      );
    }
  }

  start(
    file: string,
    name: string,
    cwd: string,
    command: string,
    bootMs: number,
  ): void {
    const terminal = vscode.window.createTerminal({ name, cwd });
    terminal.show(true);
    terminal.sendText(command);
    this.starting.set(file, { terminal, until: Date.now() + bootMs });
  }

  isStarting(file: string): boolean {
    const launch = this.starting.get(file);
    if (launch && Date.now() < launch.until) return true;
    this.starting.delete(file);
    return false;
  }

  forget(file: string): void {
    this.starting.delete(file);
  }

  async whenUp<T>(
    file: string,
    probe: () => Promise<T | undefined>,
    lost: () => Promise<boolean> = async () => false,
  ): Promise<T | undefined> {
    for (;;) {
      const answer = await probe();
      if (answer !== undefined) {
        this.forget(file);
        return answer;
      }
      if (await lost()) {
        this.forget(file);
        return undefined;
      }
      if (!this.isStarting(file)) return undefined;
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
  }

  private forgetTerminal(terminal: vscode.Terminal): void {
    for (const [file, launch] of this.starting) {
      if (launch.terminal === terminal) this.starting.delete(file);
    }
  }

  dispose(): void {
    for (const subscription of this.subscriptions) subscription.dispose();
  }
}

const POLL_MS = 500;

// The channel opens before the session builds anything, after its bake and transpile.
const WINDOW_BOOT_MS = 3 * 60 * 1000;

export class WindowWorkshops {
  constructor(private readonly launches: Launches) {}

  async show(target: PreviewTarget): Promise<void> {
    const { workshops } = target;
    if (workshops.length === 0) return noWorkshopFor(target.packageDir);
    for (const workshop of workshops) {
      if (await goto(workshop, target.location)) return;
    }
    const booting = workshops.find((workshop) =>
      this.launches.isStarting(workshop.channel),
    );
    if (booting) {
      await this.launches.whenUp(booting.channel, async () =>
        (await goto(booting, target.location)) ? true : undefined,
      );
      return;
    }
    const [host] = workshops;
    this.launches.start(
      host.channel,
      "Telar workshop",
      host.packageDir,
      `cargo telar preview ${target.id}`,
      WINDOW_BOOT_MS,
    );
  }

  async open(workshops: Workshop[], packageDir: string): Promise<void> {
    if (workshops.length === 0) return noWorkshopFor(packageDir);
    for (const workshop of workshops) {
      if (await focus(workshop)) return;
    }
    if (
      workshops.some((workshop) => this.launches.isStarting(workshop.channel))
    )
      return;
    const [host] = workshops;
    this.launches.start(
      host.channel,
      "Telar workshop",
      host.packageDir,
      "cargo telar preview",
      WINDOW_BOOT_MS,
    );
  }

  async follow(target: PreviewTarget): Promise<boolean> {
    for (const workshop of target.workshops) {
      if (await goto(workshop, target.location)) return true;
    }
    return false;
  }
}

export function noWorkshopFor(packageDir: string): void {
  void vscode.window.showInformationMessage(
    `No application in this workspace shows the previews of ${packageDir}: a library's previews are shown by an application that names the library under [telar.previews] include in its telar.toml.`,
  );
}
