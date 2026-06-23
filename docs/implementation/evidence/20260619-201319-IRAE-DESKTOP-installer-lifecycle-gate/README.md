# Installer Lifecycle Gate

Generated: 2026-06-19T11:13:20.7857159Z

This gate is non-mutating. It records the current Windows installer lifecycle
state so manual install/uninstall passes can leave auditable evidence.

- Expected phase: installed
- Require CLI: False
- Result: passed
- Installer found: True
- Registry entry present: True
- Installed executable present: True
- Installed agentmux.exe present: False
- Installed cmux.exe present: False
- Uninstall command present: True
- Shortcuts present: True
