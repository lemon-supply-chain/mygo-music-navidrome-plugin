import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import type { Host, RedirectOptions } from "../src/types";

export interface HarnessSession {
  url: string;
  username: string;
  password: string;
  name?: string;
}

export interface HarnessOptions {
  initialStorage?: Record<string, any>;
  // What the sign-in helper reports, as if the page had been used.
  session?: HarnessSession;
  onSpawn?: (command: string, args: string[]) => void;
}

export function createHarness(options: HarnessOptions = {}) {
  const storage: Record<string, any> = { ...options.initialStorage };
  const spawns: { command: string; args: string[] }[] = [];
  const opened: string[] = [];
  const redirects: RedirectOptions[] = [];
  // What the page is handed when Done is clicked, as base64 JSON in the query.
  const session = options.session ?? { url: "http://127.0.0.1:1", username: "me", password: "secret" };
  const answer = Buffer.from(
    JSON.stringify({
      name: session.name || session.username,
      url: session.url,
      username: session.username,
      password: session.password,
    }),
  ).toString("base64url");

  // Whoever waits on the helper's callback port.
  let waiting: ((query: Record<string, string>) => void) | null = null;

  const hostMock: Host = {
    platform: "darwin",
    arch: "arm64",
    dir: "/tmp",
    dataDir: "/tmp",
    setting: () => undefined,
    storage: {
      get: (key: string) => storage[key],
      set: (key: string, value: any) => {
        storage[key] = value;
      },
      remove: (key: string) => {
        delete storage[key];
      },
    },
    crypto: {
      md5: (text: string) => createHash("md5").update(text).digest("hex"),
    },
    t: (text: string) => text,
    language: () => "en",
    openURL: (url: string) => {
      opened.push(url);
      // Opening the helper's page is clicking Done right away, in the browser.
      const settle = waiting;
      waiting = null;
      settle?.({ data: answer });
    },
    waitForRedirect: (request: RedirectOptions) => {
      redirects.push(request);
      return new Promise<Record<string, string>>((resolve) => {
        waiting = resolve;
      });
    },
    spawn: (command: string, args: string[]) => {
      spawns.push({ command, args });
      options.onSpawn?.(command, args);

      if (command.startsWith("bin/navidrome-login")) {
        const lines = [
          JSON.stringify({
            type: "ready",
            url: "http://127.0.0.1:4599/?token=test",
            callback: "http://127.0.0.1:4601/navidrome",
          }),
        ];
        // A helper waits for its Done button until it is killed, like the real one.
        let stopped = false;
        let parked: (() => void) | null = null;
        return {
          closeStdin() {},
          async readLine() {
            if (lines.length > 0) return lines.shift()!;
            if (stopped) return null;
            await new Promise<void>((resume) => (parked = resume));
            return null;
          },
          wait: async () => 0,
          errorOutput: () => "",
          kill() {
            stopped = true;
            parked?.();
            parked = null;
          },
        };
      }

      const version = args[0] === "-version";
      let reads = 0;
      return {
        closeStdin() {},
        async readLine() {
          return null;
        },
        wait() {
          return new Promise((res) => setTimeout(() => res(0), version ? 0 : 500));
        },
        errorOutput() {
          if (version) return "ffmpeg version 9.0.2\n";
          reads++;
          const line = "  Stream #0:0[0x1](und): Audio: flac, 48000 Hz, stereo, s32 (24 bit)";
          return "    LYRICS: " + "x".repeat(8192) + "\n" + (reads < 2 ? line.slice(0, 20) : line + "\n");
        },
        kill() {},
      };
    },
  };

  (globalThis as any).host = hostMock;

  return {
    host: hostMock,
    storage,
    spawns,
    opened,
    redirects,
  };
}

export async function loadBuiltPlugin(root: string = process.cwd()) {
  const file = resolve(root, "dist/plugin/index.js");
  const source = await readFile(file, "utf8");
  return import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
}
