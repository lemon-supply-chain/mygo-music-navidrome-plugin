// The account in use, and the local sign-in page that chooses it.

import { DEFAULT_IMAGE } from "./constants";
import type { Account, HostProcess, Session } from "./types";
import { base64Text, fail } from "./utils";

const KEY = "session";

// The helper has no window of its own: it prints the page's URL and a free
// callback port, the plugin opens the page in a browser and listens on that
// port, and Done answers it with the account as base64 JSON in the query.
const CALLBACK_TIMEOUT = 5 * 60 * 1000;

// A helper that never says where its page is has no page to show.
const STARTUP = 8000;

type SignIn = {
  proc: HostProcess | null;
  reject: (error: Error) => void;
};

let active: SignIn | null = null;

export function current(): Session | null {
  const value = host.storage.get(KEY);
  if (!value || typeof value !== "object") return null;
  const { url, username, password } = value;
  if (typeof url !== "string" || typeof username !== "string" || typeof password !== "string") return null;
  const name = typeof value.name === "string" && value.name ? value.name : value.label;
  return {
    id: String(value.id || username),
    name: typeof name === "string" && name ? name : username,
    url,
    username,
    password,
  };
}

export function save(session: Session) {
  host.storage.set(KEY, session);
}

export function clear() {
  host.storage.remove(KEY);
}

export function account(): Account | null {
  const session = current();
  if (!session) return null;
  return { id: session.username, name: session.name, imageUrl: DEFAULT_IMAGE };
}

export function signIn(): Promise<Account> {
  replace();
  return new Promise<Account>((resolve, reject) => {
    const attempt: SignIn = { proc: null, reject };
    active = attempt;
    void run(attempt).then(
      (session) => {
        if (active !== attempt) return;
        active = null;
        save(session);
        resolve(accountOf(session));
      },
      (error) => {
        if (active !== attempt) return;
        active = null;
        reject(error);
      },
    );
  });
}

export function stop() {
  const attempt = active;
  active = null;
  attempt?.proc?.kill();
  attempt?.reject(fail("signIn", host.t("Signed out.")));
}

function replace() {
  const previous = active;
  active = null;
  previous?.proc?.kill();
  previous?.reject(fail("signIn", host.t("The sign-in page was replaced.")));
}

async function run(attempt: SignIn): Promise<Session> {
  if (typeof host.waitForRedirect !== "function" || typeof host.openURL !== "function") {
    throw fail("signIn", host.t("This version of MyGo Music can't finish a sign-in."));
  }

  const proc = spawn();
  attempt.proc = proc;
  if (active !== attempt) {
    proc.kill();
    throw fail("signIn", host.t("The sign-in page was replaced."));
  }

  const page = follow(proc);
  try {
    const ready = await withTimeout(page.ready, STARTUP, () => fail("signIn", host.t("The sign-in page didn't open.")));
    if (active !== attempt) {
      throw fail("signIn", host.t("The sign-in page was replaced."));
    }

    // The helper found a free port for the callback. Listening starts before
    // the page opens, so nothing has to guess a port.
    const answer = new URL(ready.callback);
    const redirect = host.waitForRedirect({
      port: Number(answer.port),
      path: answer.pathname,
      timeout: CALLBACK_TIMEOUT,
    });

    host.openURL(ready.url);
    const query = await Promise.race([redirect, page.closed]);
    return sessionOf(query);
  } finally {
    proc.kill();
  }
}

function spawn(): HostProcess {
  const names = helperNames();
  let missing: unknown = null;
  for (const name of names) {
    try {
      return host.spawn(name, ["--dir", host.dataDir, "--port", "0"]);
    } catch (error) {
      missing = error;
    }
  }
  throw fail(
    "signIn",
    host.t("This build has no sign-in page for this platform.") +
      " (" +
      names.join(", ") +
      " " +
      String(missing ?? "") +
      ")",
  );
}

// amd64 is Go's name for x64, and both turn up in the wild.
function helperNames(): string[] {
  const arches = host.arch === "x64" || host.arch === "amd64" ? ["amd64", "x64"] : [host.arch];
  const suffix = host.platform === "windows" ? ".exe" : "";
  return arches.map((arch) => `bin/navidrome-login-${host.platform}-${arch}${suffix}`);
}

type Ready = { url: string; callback: string };

type HelperEvent = { type?: string; url?: string; callback?: string; message?: string };

// Reads the helper's lines: the page URL first, then why it stopped, if it
// stops before the browser comes back.
function follow(proc: HostProcess) {
  let ready = false;
  let settleClosed!: (error: Error) => void;
  const closed = new Promise<never>((_, reject) => (settleClosed = reject));
  // Whoever wins the race stops caring; this one keeps the runtime quiet.
  closed.catch(() => {});

  const page = new Promise<Ready>((resolve, reject) => {
    void (async () => {
      try {
        let line: string | null;
        while ((line = await proc.readLine()) !== null) {
          let event: HelperEvent;
          try {
            event = JSON.parse(line);
          } catch {
            continue;
          }
          if (!ready && event.type === "ready" && typeof event.url === "string" && typeof event.callback === "string") {
            ready = true;
            resolve({ url: event.url, callback: event.callback });
            continue;
          }
          if (event.type === "error" && typeof event.message === "string") {
            const error = fail(ready ? "signIn" : "setup", event.message);
            if (!ready) reject(error);
            settleClosed(error);
            return;
          }
        }
      } catch (error) {
        const reason = fail("signIn", String(error));
        if (!ready) reject(reason);
        settleClosed(reason);
        return;
      }
      const message = ready
        ? host.t("The sign-in page closed before a server was chosen.")
        : host.t("The sign-in page didn't start.");
      if (!ready) reject(fail("setup", message));
      settleClosed(fail("signIn", message));
    })();
  });

  return { ready: page, closed };
}

function sessionOf(query: Record<string, string>): Session {
  const data = query && typeof query.data === "string" ? query.data : "";
  if (!data) throw fail("signIn", host.t("The sign-in page didn't hand back a server."));

  let value: any;
  try {
    value = JSON.parse(base64Text(data));
  } catch {
    throw fail("signIn", host.t("The sign-in page handed back something unreadable."));
  }

  const url = typeof value?.url === "string" ? value.url : "";
  const username = typeof value?.username === "string" ? value.username : "";
  const password = typeof value?.password === "string" ? value.password : "";
  const label = typeof value?.label === "string" && value.label ? value.label : value?.name;
  const name = typeof label === "string" && label ? label : url;
  const id = typeof value?.id === "string" && value.id ? value.id : username;
  if (!url || !username || !password) {
    throw fail("signIn", host.t("The sign-in page handed back something incomplete."));
  }
  return { id, name, url, username, password };
}

function accountOf(session: Session): Account {
  return { id: session.username, name: session.name, imageUrl: DEFAULT_IMAGE };
}

function withTimeout<T>(promise: Promise<T>, ms: number, error: () => Error): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(error()), ms);
    promise.then(
      (value) => {
        clearTimeout(timer);
        resolve(value);
      },
      (reason) => {
        clearTimeout(timer);
        reject(reason);
      },
    );
  });
}
