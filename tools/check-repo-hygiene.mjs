import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();

function listRepositoryFiles() {
  try {
    return execFileSync(
      "git",
      ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
      {
        cwd: root,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      },
    )
      .split("\0")
      .filter((file) => file && !normalizePath(file).startsWith(".tmp/"));
  } catch (cause) {
    throw new Error("Unable to enumerate repository files for hygiene checks.", {
      cause,
    });
  }
}

function isBinary(buffer) {
  return buffer.includes(0);
}

function normalizePath(value) {
  return value.replaceAll("\\", "/");
}

const repositoryFiles = listRepositoryFiles();
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

function isDesktopProductSource(file) {
  const normalized = normalizePath(file);
  if (!normalized.startsWith("apps/desktop/src/")) {
    return false;
  }

  if (!/\.[cm]?[jt]sx?$/.test(normalized)) {
    return false;
  }

  return !(
    normalized.includes("/__tests__/") ||
    normalized.includes("/test/") ||
    /(?:^|\.)(?:spec|test)\.[cm]?[jt]sx?$/.test(path.basename(normalized))
  );
}

function findLineNumber(text, index) {
  return text.slice(0, index).split("\n").length;
}

function findNativeDialogCalls(text) {
  const matches = [];
  const candidatePattern = /\b(?:alert|confirm|prompt)\s*\(/g;
  let match;
  while ((match = candidatePattern.exec(text)) !== null) {
    const before = text.slice(0, match.index);
    const memberAccess = before.match(/([A-Za-z_$][\w$]*)\s*\??\.\s*$/);

    if (memberAccess) {
      const receiver = memberAccess[1];
      if (
        receiver !== "window" &&
        receiver !== "globalThis" &&
        receiver !== "self"
      ) {
        continue;
      }
    } else {
      const linePrefix = before.slice(before.lastIndexOf("\n") + 1).trim();
      const parameterTail = text.slice(candidatePattern.lastIndex);
      const isDeclaration =
        /(?:^|\s)(?:function|declare)\s*$/.test(linePrefix) ||
        (linePrefix === "" &&
          /^\s*[A-Za-z_$][\w$]*\s*[?:]\s*/.test(parameterTail));
      if (isDeclaration) {
        continue;
      }
    }

    matches.push(match.index);
  }
  return matches;
}

const nativeDialogRuleFixtures = [
  { source: "window.confirm('delete?')", expected: 1 },
  { source: "globalThis.prompt('value')", expected: 1 },
  { source: "window?.alert('failed')", expected: 1 },
  { source: "self.confirm('continue?')", expected: 1 },
  { source: "alert('failed')", expected: 1 },
  { source: "dialogs.confirm({ title: 'Safe' })", expected: 0 },
  { source: "confirm(options: ConfirmDialogOptions): Promise<boolean>;", expected: 0 },
  { source: "function prompt(value) { return value; }", expected: 0 },
];
for (const fixture of nativeDialogRuleFixtures) {
  const actual = findNativeDialogCalls(fixture.source).length;
  if (actual !== fixture.expected) {
    failures.push(
      `internal native-dialog rule fixture failed (${JSON.stringify(fixture.source)}: expected ${fixture.expected}, received ${actual})`,
    );
  }
}

function requireFileText(relativeFile) {
  const absoluteFile = path.join(root, relativeFile);
  if (!fs.existsSync(absoluteFile)) {
    failures.push(`${relativeFile}: required file is missing`);
    return "";
  }
  return fs.readFileSync(absoluteFile, "utf8");
}

function requireText(text, relativeFile, description, pattern) {
  if (!pattern.test(text)) {
    failures.push(`${relativeFile}: missing ${description}`);
  }
}

for (const relativeFile of repositoryFiles) {
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

  if (isDesktopProductSource(relativeFile)) {
    for (const nativeDialogIndex of findNativeDialogCalls(text)) {
      failures.push(
        `${relativeFile}:${findLineNumber(text, nativeDialogIndex)}: browser-native dialog is forbidden; use the themed app dialog service`,
      );
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

const releaseWorkflow = ".github/workflows/release.yml";
const releaseWorkflowText = requireFileText(releaseWorkflow);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "OIDC permission for GitHub Artifact Attestations",
  /^\s*id-token:\s*write\s*$/m,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "attestations write permission",
  /^\s*attestations:\s*write\s*$/m,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "artifact metadata write permission",
  /^\s*artifact-metadata:\s*write\s*$/m,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "actions/attest release step",
  /uses:\s*actions\/attest@(?:v\d+|[a-f0-9]{40})/,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "release asset subject path attestation",
  /^\s*subject-path:\s*dist\/release\/\*\s*$/m,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "post-generation attestation verification",
  /gh\s+attestation\s+verify/,
);
requireText(
  releaseWorkflowText,
  releaseWorkflow,
  "signer workflow bound attestation verification",
  /--signer-workflow\s+\$workflow/,
);

if (failures.length > 0) {
  console.error("Repository hygiene check failed:");
  for (const failure of failures) {
    console.error(`  ${failure}`);
  }
  process.exit(1);
}

console.log(`Checked ${repositoryFiles.length} repository files for hygiene.`);
