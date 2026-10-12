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

function readServerFileNow(file: string): string | undefined {
  try {
    return fs.readFileSync(file, "utf8").trim() || undefined;
  } catch {
    return undefined;
  }
}

function isLiveStart(text: string | undefined): boolean {
  const state = text === undefined ? undefined : parseServerFile(text);
  return state?.kind === "starting" && isAlive(state.pid);
}

// Follows what the file a session names itself in says from now on. What it held when following began is a leftover of an earlier session, unless it names a live session that is still starting, which is then the session followed.
class SessionFile {
  private constructor(
    private readonly file: string,
    private readonly initial: string | undefined,
    private announced: boolean,
  ) {}

  // Read at once, so a launch is known before anything else can start one.
  static follow(file: string): SessionFile {
    const initial = readServerFileNow(file);
    return new SessionFile(file, initial, isLiveStart(initial));
  }

  get hasAnnounced(): boolean {
    return this.announced;
  }

  // Whether the session is gone: it announced itself, then removed its file or left it naming a process that has died.
  async gone(): Promise<boolean> {
    const text = await readServerFile(this.file);
    if (text !== undefined && text !== this.initial) this.announced = true;
    if (!this.announced) return false;
    if (text === undefined) return true;
    const state = parseServerFile(text);
    return state?.kind === "starting" && !isAlive(state.pid);
  }
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

export type Stopped =
  | { kind: "exited"; code: number | undefined }
  | { kind: "closed" }
  | { kind: "lost" }
  | { kind: "silent" }
  | { kind: "timeout" };

export type Outcome<T> = { up: T } | { down: Stopped };

interface Launch {
  // Absent for a session started elsewhere, which is followed through its file alone.
  terminal?: vscode.Terminal;
  session: SessionFile;
  until: number;
  announceBy: number;
  ended?: Stopped;
}

// A shell that reports its commands says when the launched one ends; without that, a session that has not named itself in its file by then is taken for one that never started, such as a `cargo` the shell cannot find.
const ANNOUNCE_MS = 60 * 1000;

// What VS Code's own sample waits for shell integration before falling back to typing the command.
const SHELL_INTEGRATION_MS = 3000;

// Keyed by the file each session writes once it is up, so a request made while one boots waits for it instead of starting a second.
export class Launches implements vscode.Disposable {
  private readonly starting = new Map<string, Launch>();
  private readonly subscriptions: vscode.Disposable[] = [];

  constructor() {
    this.subscriptions.push(
      vscode.window.onDidCloseTerminal((terminal) =>
        this.end(terminal, { kind: "closed" }),
      ),
    );
    // Older editors have no such event, and a shell without integration never fires it.
    const ended = vscode.window.onDidEndTerminalShellExecution as
      typeof vscode.window.onDidEndTerminalShellExecution | undefined;
    if (ended) {
      this.subscriptions.push(
        ended((event) =>
          this.end(event.terminal, { kind: "exited", code: event.exitCode }),
        ),
      );
    }
  }

  async start(
    file: string,
    name: string,
    cwd: string,
    command: string,
    bootMs: number,
  ): Promise<void> {
    const session = SessionFile.follow(file);
    const terminal = vscode.window.createTerminal({ name, cwd });
    terminal.show(true);
    const now = Date.now();
    this.starting.set(file, {
      terminal,
      session,
      until: now + bootMs,
      announceBy: now + ANNOUNCE_MS,
    });
    const integration = await shellIntegrationOf(terminal);
    if (integration) integration.executeCommand(command);
    else terminal.sendText(command);
  }

  // Follows a session that `file` names as starting under a live process, which something other than this editor launched, and answers whether there is one.
  follow(file: string, bootMs: number): boolean {
    if (this.isStarting(file)) return true;
    const session = SessionFile.follow(file);
    if (!session.hasAnnounced) return false;
    const now = Date.now();
    this.starting.set(file, { session, until: now + bootMs, announceBy: now });
    return true;
  }

  isStarting(file: string): boolean {
    const launch = this.starting.get(file);
    if (launch && !launch.ended && !this.overdue(launch)) return true;
    this.starting.delete(file);
    return false;
  }

  forget(file: string): void {
    this.starting.delete(file);
  }

  async whenUp<T>(
    file: string,
    probe: () => Promise<T | undefined>,
  ): Promise<Outcome<T>> {
    let launch = this.starting.get(file);
    for (;;) {
      const answer = await probe();
      if (answer !== undefined) {
        this.forget(file);
        return { up: answer };
      }
      const stopped: Stopped | undefined = launch
        ? await this.stopped(launch)
        : { kind: "lost" };
      if (stopped) {
        if (this.starting.get(file) === launch) this.starting.delete(file);
        const remaining = (launch?.until ?? 0) - Date.now();
        // A second launch ends at once when another session got there first, which is then the one to wait for.
        if (launch?.terminal && remaining > 0 && this.follow(file, remaining)) {
          launch = this.starting.get(file);
          continue;
        }
        return { down: stopped };
      }
      await new Promise((resolve) => setTimeout(resolve, POLL_MS));
    }
  }

  private async stopped(launch: Launch): Promise<Stopped | undefined> {
    if (launch.ended) return launch.ended;
    if (await launch.session.gone()) return { kind: "lost" };
    if (Date.now() >= launch.until) return { kind: "timeout" };
    if (this.overdue(launch)) return { kind: "silent" };
    return undefined;
  }

  private overdue(launch: Launch): boolean {
    const now = Date.now();
    if (now >= launch.until) return true;
    const unobserved =
      launch.terminal !== undefined &&
      launch.terminal.shellIntegration === undefined;
    return (
      unobserved && !launch.session.hasAnnounced && now >= launch.announceBy
    );
  }

  private end(terminal: vscode.Terminal, why: Stopped): void {
    for (const launch of this.starting.values()) {
      if (launch.terminal === terminal) launch.ended ??= why;
    }
  }

  dispose(): void {
    for (const subscription of this.subscriptions) subscription.dispose();
  }
}

function shellIntegrationOf(
  terminal: vscode.Terminal,
): Promise<vscode.TerminalShellIntegration | undefined> {
  if (terminal.shellIntegration)
    return Promise.resolve(terminal.shellIntegration);
  const changed = vscode.window.onDidChangeTerminalShellIntegration as
    typeof vscode.window.onDidChangeTerminalShellIntegration | undefined;
  if (!changed) return Promise.resolve(undefined);
  return new Promise((resolve) => {
    const settle = (integration?: vscode.TerminalShellIntegration) => {
      clearTimeout(timer);
      subscription.dispose();
      resolve(integration);
    };
    const subscription = changed((event) => {
      if (event.terminal === terminal) settle(event.shellIntegration);
    });
    const timer = setTimeout(() => settle(), SHELL_INTEGRATION_MS);
  });
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
    await this.launches.start(
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
    await this.launches.start(
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
