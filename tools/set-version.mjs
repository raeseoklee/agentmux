#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = path.resolve(import.meta.dirname, "..");
const semverPattern = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

function usage() {
  return "Usage: npm run version:set -- <semver>";
}

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));
}

function writeJson(relativePath, value) {
  fs.writeFileSync(
    path.join(root, relativePath),
    `${JSON.stringify(value, null, 2)}\n`,
    "utf8",
  );
}

function updateJsonVersion(relativePath, version) {
  const value = readJson(relativePath);
  value.version = version;
  writeJson(relativePath, value);
}

function updateDesktopLockVersion(version) {
  const relativePath = "apps/desktop/package-lock.json";
  const absolutePath = path.join(root, relativePath);
  if (!fs.existsSync(absolutePath)) {
    return;
  }
  const lock = readJson(relativePath);
  lock.version = version;
  if (lock.packages?.[""]) {
    lock.packages[""].version = version;
  }
  if (lock.packages?.["../.."]) {
    lock.packages["../.."].version = version;
  }
  writeJson(relativePath, lock);
}

function updateWorkspaceCargoVersion(version) {
  const cargoPath = path.join(root, "Cargo.toml");
  const text = fs.readFileSync(cargoPath, "utf8");
  const next = text.replace(
    /(\[workspace\.package\][\s\S]*?^version\s*=\s*")([^"]+)(")/m,
    `$1${version}$3`,
  );
  if (next === text) {
    throw new Error("Cargo.toml is missing [workspace.package] version.");
  }
  fs.writeFileSync(cargoPath, next, "utf8");
}

const version = process.argv[2]?.replace(/^v/, "");
if (!version || process.argv.length > 3) {
  console.error(usage());
  process.exit(1);
}
if (!semverPattern.test(version)) {
  console.error(`Invalid AgentMux version '${version}'. Expected SemVer such as 0.1.1 or 0.2.0-rc.1.`);
  process.exit(1);
}

updateJsonVersion("package.json", version);
updateJsonVersion("apps/desktop/package.json", version);
updateJsonVersion("apps/desktop/src-tauri/tauri.conf.json", version);
updateDesktopLockVersion(version);
updateWorkspaceCargoVersion(version);

console.log(`AgentMux version set to ${version}`);
