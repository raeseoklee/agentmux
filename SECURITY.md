# Security Policy

## Reporting a Vulnerability

Please do not file public issues for vulnerabilities.

Until a dedicated security advisory channel is configured, send security reports
privately to the repository maintainers. Include:

- Affected version or commit.
- Operating system and shell/backend used.
- Reproduction steps.
- Expected and actual behavior.
- Any relevant logs with secrets, tokens, hostnames, and private paths removed.

We will acknowledge valid reports, investigate them privately, and publish fixes
with release notes when disclosure is appropriate.

## Sensitive Data

AgentMux controls local shells, WSL sessions, tmux sessions, SSH sessions, and
browser surfaces. Do not share diagnostics or terminal output publicly until you
have checked them for tokens, command history, private paths, hostnames, and
project data.
