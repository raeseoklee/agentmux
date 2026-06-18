import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const markdownFiles = [];

function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (
      entry.name === ".git" ||
      entry.name === ".toolchains" ||
      entry.name === "node_modules" ||
      entry.name === "target" ||
      entry.name === "dist" ||
      entry.name === "build"
    ) {
      continue;
    }

    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(fullPath);
    } else if (entry.isFile() && entry.name.endsWith(".md")) {
      markdownFiles.push(fullPath);
    }
  }
}

function stripAnchor(link) {
  const hashIndex = link.indexOf("#");
  return hashIndex === -1 ? link : link.slice(0, hashIndex);
}

function isExternal(link) {
  return /^[a-zA-Z][a-zA-Z\d+.-]*:/.test(link) || link.startsWith("mailto:");
}

walk(root);

const failures = [];
const linkPattern = /\[[^\]]+\]\(([^)]+)\)/g;

for (const file of markdownFiles) {
  const text = fs.readFileSync(file, "utf8");
  let match;
  while ((match = linkPattern.exec(text)) !== null) {
    const rawLink = match[1].trim();
    if (!rawLink || isExternal(rawLink)) {
      continue;
    }

    const withoutAnchor = stripAnchor(rawLink);
    if (!withoutAnchor) {
      continue;
    }

    const decoded = decodeURI(withoutAnchor);
    const target = path.resolve(path.dirname(file), decoded);
    if (!fs.existsSync(target)) {
      failures.push(`${path.relative(root, file)} -> ${rawLink}`);
    }
  }
}

if (failures.length > 0) {
  console.error("Broken markdown links:");
  for (const failure of failures) {
    console.error(`  ${failure}`);
  }
  process.exit(1);
}

console.log(`Checked ${markdownFiles.length} markdown files; no broken local links found.`);
