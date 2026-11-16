import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { unzipSync, zipSync } from "fflate";
import { runBuild } from "./build";
import { validateMetadata } from "./metadata";

type Files = Record<string, [Uint8Array, { mtime: Date }]>;

// The app keeps the executable bit from the archive, so the helper needs one.
const UNIX = 3;
const MODE_FILE = { os: UNIX, attrs: ((0o100644 << 16) | 0x20) >>> 0 };
const MODE_EXEC = { os: UNIX, attrs: ((0o100755 << 16) | 0x20) >>> 0 };

// One archive per platform, named as the app looks for it: plugin-darwin-arm64.zip.
const HELPER = /^bin\/navidrome-login-(darwin|linux|windows)-(arm64|amd64|x64)(\.exe)?$/;

function collectFiles(dir: string, base: string = dir): Files {
  const result: Files = {};
  const entries = readdirSync(dir).sort();
  for (const entry of entries) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      Object.assign(result, collectFiles(full, base));
    } else {
      const rel = relative(base, full).split("\\").join("/");
      result[rel] = [new Uint8Array(readFileSync(full)), { mtime: new Date("1980-01-01T00:00:00Z") }];
    }
  }
  return result;
}

function sorted(files: Files): Files {
  const out: Files = {};
  for (const key of Object.keys(files).sort()) out[key] = files[key];
  return out;
}

function platformOf(rel: string): string | null {
  const match = HELPER.exec(rel);
  if (!match) return null;
  const arch = match[2] === "x64" ? "amd64" : match[2];
  return `${match[1]}-${arch}`;
}

function platformsOf(files: Files): string[] {
  const found = new Set<string>();
  for (const rel of Object.keys(files)) {
    const platform = platformOf(rel);
    if (platform) found.add(platform);
  }
  return [...found].sort();
}

function sha256(bytes: Uint8Array) {
  return createHash("sha256").update(bytes).digest("hex");
}

export async function runPackage(root: string = process.cwd()) {
  const meta = validateMetadata(root);

  await runBuild(root);
  const pluginDir = resolve(root, "dist/plugin");
  const first = collectFiles(pluginDir);
  const firstZip = zipSync(first);

  await runBuild(root);
  const second = zipSync(collectFiles(pluginDir));
  if (sha256(firstZip) !== sha256(second)) {
    throw new Error(`Non-deterministic build detected: ${sha256(firstZip)} !== ${sha256(second)}`);
  }

  const unzipped = unzipSync(firstZip);
  const required = ["index.js", "plugin.json", "locales/zh-CN.json", "README.md"];
  for (const file of required) {
    if (!unzipped[file]) {
      throw new Error(`Archive missing required file: ${file}`);
    }
  }

  const indexSource = Buffer.from(unzipped["index.js"]).toString("base64");
  const mod = await import(`data:text/javascript;base64,${indexSource}`);
  const actualExports = Object.keys(mod).sort();
  const expectedExports: string[] = JSON.parse(
    readFileSync(resolve(root, "tests/fixtures/exports.json"), "utf8"),
  ).sort();

  if (JSON.stringify(actualExports) !== JSON.stringify(expectedExports)) {
    throw new Error("Unpacked archive exports mismatch fixture");
  }

  const releaseDir = resolve(root, "dist/release");
  mkdirSync(releaseDir, { recursive: true });

  const targets = platformsOf(first);
  const archives = targets.length > 0 ? targets : [""];

  for (const target of archives) {
    const files: Files = {};
    for (const [rel, value] of Object.entries(first)) {
      const platform = platformOf(rel);
      if (platform && platform !== target) continue;
      files[rel] = value;
    }

    const entries: Record<string, [Uint8Array, { mtime: Date; os: number; attrs: number }]> = {};
    for (const [rel, [data, options]] of Object.entries(sorted(files))) {
      const mode = platformOf(rel) ? MODE_EXEC : MODE_FILE;
      entries[rel] = [data, { mtime: options.mtime, os: mode.os, attrs: mode.attrs }];
    }
    const zip = zipSync(entries);
    const zipName = target ? `plugin-${target}.zip` : `${meta.id}-v${meta.version}.zip`;
    writeFileSync(resolve(releaseDir, zipName), zip);
    writeFileSync(resolve(releaseDir, `${zipName}.sha256`), `${sha256(zip)}  ${zipName}\n`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(process.cwd(), "scripts/package.ts")) {
  runPackage().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
