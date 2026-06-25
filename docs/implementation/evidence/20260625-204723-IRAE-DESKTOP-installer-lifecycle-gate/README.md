# Installer Lifecycle Gate

Generated: 2026-06-25T11:47:25.7214253Z

This gate is non-mutating. It records the current Windows installer lifecycle
state so manual install/uninstall passes can leave auditable evidence.

- Expected phase: installed
- Require CLI: True
- Require user PATH: True
- Result: passed
- Installer found: True
- Registry entry present: True
- Installed executable present: True
- Installed agentmux.exe present: True
- Installed cmux.exe present: True
- Install directory on user PATH: True
- Uninstall command present: True
- Shortcuts present: True
