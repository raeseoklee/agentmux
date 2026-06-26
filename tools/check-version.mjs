#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = path.resolve(import.meta.dirname, "..");
const semverPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));
}

function readWorkspaceCargoVersion() {
  const text = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const match = text.match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("Cargo.toml is missing [workspace.package] version.");
  }
  return match[1];
}

function expectedVersionFromArgs(args) {
  let expected = null;
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--version" || arg === "--tag") {
      const value = args[index + 1];
      if (!value) {
        throw new Error(`${arg} requires a value.`);
      }
      expected = arg === "--tag" ? value.replace(/^v/, "") : value;
      index += 1;
    } else if (arg.startsWith("--version=")) {
      expected = arg.slice("--version=".length);
    } else if (arg.startsWith("--tag=")) {
      expected = arg.slice("--tag=".length).replace(/^v/, "");
    } else {
      throw new Error(`Unknown option '${arg}'.`);
    }
  }
  return expected;
}

const versions = new Map([
  ["package.json", readJson("package.json").version],
  ["apps/desktop/package.json", readJson("apps/desktop/package.json").version],
  ["apps/desktop/src-tauri/tauri.conf.json", readJson("apps/desktop/src-tauri/tauri.conf.json").version],
  ["Cargo.toml workspace.package", readWorkspaceCargoVersion()],
]);

const lockPath = "apps/desktop/package-lock.json";
if (fs.existsSync(path.join(root, lockPath))) {
  const lock = readJson(lockPath);
  versions.set(lockPath, lock.version);
  versions.set(`${lockPath} packages[""]`, lock.packages?.[""]?.version);
  versions.set(`${lockPath} packages["../.."]`, lock.packages?.["../.."]?.version);
}

const expected = expectedVersionFromArgs(process.argv.slice(2));
const distinct = new Set(versions.values());
const invalid = [...versions.entries()].filter(([, version]) => !semverPattern.test(version ?? ""));
const mismatched = expected
  ? [...versions.entries()].filter(([, version]) => version !== expected)
  : [];

if (invalid.length > 0 || distinct.size !== 1 || mismatched.length > 0) {
  console.error("Version check failed:");
  for (const [source, version] of versions) {
    const suffix = version === expected || !expected ? "" : " (expected tag/version mismatch)";
    console.error(`  ${source}: ${version}${suffix}`);
  }
  if (expected) {
    console.error(`  expected: ${expected}`);
  }
  process.exit(1);
}

const [version] = distinct;
console.log(`AgentMux version check passed: ${version}`);
