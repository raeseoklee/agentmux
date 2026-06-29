# Security Policy

## Reporting a Vulnerability

Please do not file public issues for vulnerabilities.

Use GitHub private vulnerability reporting when it is available on this
repository. If private reporting is unavailable, contact the repository
maintainer privately before publishing details. Include:

- Affected version or commit.
- Operating system and shell/backend used.
- Reproduction steps.
- Expected and actual behavior.
- Any relevant logs with secrets, tokens, hostnames, and private paths removed.

We will acknowledge valid reports, investigate them privately, and publish fixes
with release notes when disclosure is appropriate.

## Security Scope

The active release scope is the Windows desktop app, bundled CLI, local control
plane, WSL integration, tmux integration, SSH backend, browser automation, and
server mode.

AgentMux is a local developer tool. Same-user malware, compromised shells,
compromised agent CLIs, and secrets intentionally printed into terminal output
are out of scope.

## Sensitive Data

AgentMux controls local shells, WSL sessions, tmux sessions, SSH sessions, and
browser surfaces. Do not share diagnostics or terminal output publicly until you
have checked them for tokens, command history, private paths, hostnames, and
project data.
