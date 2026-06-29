import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();

function listTrackedFiles() {
  try {
    return execFileSync("git", ["ls-files", "-z"], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .split("\0")
      .filter(Boolean);
  } catch {
    return [];
  }
}

function isBinary(buffer) {
  return buffer.includes(0);
}

function normalizePath(value) {
  return value.replaceAll("\\", "/");
}

const trackedFiles = listTrackedFiles();
const failures = [];
const markdownLinkPattern = /\[[^\]]+\]\(([^)]+)\)/g;
const personalPathPatterns = [
  {
    name: "local workspace path",
    pattern: /D:\\+Workspace\\+irae/i,
  },
  {
    name: "local WSL workspace path",
    pattern: /\/mnt\/d\/Workspace\/irae/i,
  },
  {
    name: "local user profile path",
    pattern: /C:\\+Users\\+irae/i,
  },
];

function isExternalLink(link) {
  return /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(link) || link.startsWith("mailto:");
}

function stripAnchor(link) {
  const hashIndex = link.indexOf("#");
  return hashIndex === -1 ? link : link.slice(0, hashIndex);
}

function isPublicDoc(file) {
  const normalized = normalizePath(file);
  return normalized.startsWith("docs/en/") || normalized.startsWith("docs/ko/");
}

for (const relativeFile of trackedFiles) {
  const absoluteFile = path.join(root, relativeFile);
  if (!fs.existsSync(absoluteFile)) {
    continue;
  }

  const buffer = fs.readFileSync(absoluteFile);
  if (isBinary(buffer)) {
    continue;
  }

  const text = buffer.toString("utf8");
  for (const { name, pattern } of personalPathPatterns) {
    if (pattern.test(text)) {
      failures.push(`${relativeFile}: contains ${name}`);
    }
  }

  if (!relativeFile.endsWith(".md") || !isPublicDoc(relativeFile)) {
    continue;
  }

  let match;
  while ((match = markdownLinkPattern.exec(text)) !== null) {
    const rawLink = match[1].trim();
    if (!rawLink || isExternalLink(rawLink)) {
      continue;
    }

    const withoutAnchor = stripAnchor(rawLink);
    if (!withoutAnchor) {
      continue;
    }

    const resolved = path.relative(
      root,
      path.resolve(path.dirname(absoluteFile), decodeURI(withoutAnchor)),
    );
    const normalizedTarget = normalizePath(resolved);
    if (
      normalizedTarget.startsWith("docs/implementation/") ||
      normalizedTarget.startsWith("docs/ko/implementation/")
    ) {
      failures.push(
        `${relativeFile}: public docs must not link to private implementation docs (${rawLink})`,
      );
    }
  }
}

if (failures.length > 0) {
  console.error("Repository hygiene check failed:");
  for (const failure of failures) {
    console.error(`  ${failure}`);
  }
  process.exit(1);
}

console.log(`Checked ${trackedFiles.length} tracked files for repository hygiene.`);
