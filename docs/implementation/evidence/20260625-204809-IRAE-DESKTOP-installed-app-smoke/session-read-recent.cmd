@echo off
cd /d D:\Workspace\irae\agentmux
C:\Users\irae\AppData\Local\AgentMux\agentmux.exe session read-recent ses_00000004 --max-bytes 4096 --json --pipe \\.\pipe\agentmux-installed-smoke-079b9e77a3484a6f83e15792dc9b3cf6 --token-path D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\runtime\control.token > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\session-read-recent.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260625-204809-IRAE-DESKTOP-installed-app-smoke\session-read-recent.stderr.txt
exit /b %ERRORLEVEL%
