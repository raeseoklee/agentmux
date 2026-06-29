# Local Verification Evidence

This directory is intentionally kept out of the public git index except for this
README.

AgentMux smoke tests and release gates may write local evidence here, including
logs, JSON summaries, extracted installer contents, temporary control tokens,
runtime stores, and rebuilt installer binaries. Those artifacts can contain
machine-specific paths, usernames, hostnames, WSL distribution names, command
history, terminal output, and transient local tokens.

Do not commit generated evidence artifacts directly. For public documentation,
summarize the verification command, date, platform, and result in the relevant
release notes or operations document after removing private environment details.
