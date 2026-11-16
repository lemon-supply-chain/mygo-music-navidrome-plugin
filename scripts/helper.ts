import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";

// The app picks a zip by GOOS and GOARCH, so helpers are named with those.
type Target = { goos: string; goarch: string; triple: string };

const TARGETS: Record<string, Target[]> = {
  darwin: [
    { goos: "darwin", goarch: "arm64", triple: "aarch64-apple-darwin" },
    { goos: "darwin", goarch: "amd64", triple: "x86_64-apple-darwin" },
  ],
  linux: [
    { goos: "linux", goarch: "amd64", triple: "x86_64-unknown-linux-gnu" },
    { goos: "linux", goarch: "arm64", triple: "aarch64-unknown-linux-gnu" },
  ],
  windows: [
    { goos: "windows", goarch: "amd64", triple: "x86_64-pc-windows-msvc" },
    { goos: "windows", goarch: "arm64", triple: "aarch64-pc-windows-msvc" },
  ],
};

const crate = (root: string) => resolve(root, "native/navidrome-login");

function run(cmd: string, args: string[], options: { cwd?: string } = {}) {
  const isWin = process.platform === "win32";
  const binary = isWin && !cmd.endsWith(".exe") ? `${cmd}.exe` : cmd;
  execFileSync(binary, args, { ...options, stdio: "inherit" });
}

function rustup(target: string) {
  try {
    const isWin = process.platform === "win32";
    const binary = isWin ? "rustup.exe" : "rustup";
    execFileSync(binary, ["target", "add", target], { stdio: "ignore" });
  } catch {
    // cargo says what is missing, in more detail than rustup does.
  }
}

/** Builds the sign-in helper for this machine and copies it into bin/. */
export function buildHelpers(root: string = process.cwd()) {
  const platform = process.platform === "win32" ? "windows" : process.platform;
  const targets = TARGETS[platform] ?? [];
  if (targets.length === 0) throw new Error(`no helper targets for ${process.platform}`);

  const out = resolve(root, "bin");
  mkdirSync(out, { recursive: true });

  for (const { goos, goarch, triple } of targets) {
    rustup(triple);
    run("cargo", ["build", "--release", "--target", triple], { cwd: crate(root) });

    const exe = goos === "windows" ? "navidrome-login.exe" : "navidrome-login";
    const name = `navidrome-login-${goos}-${goarch}${goos === "windows" ? ".exe" : ""}`;
    const dest = resolve(out, name);
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(resolve(crate(root), "target", triple, "release", exe), dest);
    console.log(`bin/${name}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(process.cwd(), "scripts/helper.ts")) {
  try {
    buildHelpers();
  } catch (error) {
    console.error(error);
    process.exit(1);
  }
}
