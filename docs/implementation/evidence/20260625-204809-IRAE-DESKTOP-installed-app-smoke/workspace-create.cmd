@echo off
cd /d D:\Workspace\irae\agentmux
C:\Users\irae\AppData\Local\AgentMux\agentmux.exe workspace create InstalledSmoke --json --pipe \\.\pipe\agentmux-installed-smoke-079b9e77a3484a6f83e15792dc9b3cf6 --token-path D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\runtime\control.token > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\workspace-create.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\workspace-create.stderr.txt
exit /b %ERRORLEVEL%
