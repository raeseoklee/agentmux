@echo off
cd /d D:\Workspace\irae\agentmux
C:\Windows\system32\wsl.exe --list --quiet > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\wsl-list-quiet.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\wsl-list-quiet.stderr.txt
exit /b %ERRORLEVEL%
