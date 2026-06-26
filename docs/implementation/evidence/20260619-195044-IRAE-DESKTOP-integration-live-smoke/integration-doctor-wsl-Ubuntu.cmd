@echo off
cd /d D:\Workspace\irae\agentmux
set "PATH=D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin;%PATH%"
D:\Workspace\irae\agentmux\target\debug\cmux.exe integrations doctor --json --base-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm --bin-dir D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\runtime\cmuxterm\bin --distribution Ubuntu > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-doctor-wsl-Ubuntu.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\integration-doctor-wsl-Ubuntu.stderr.txt
exit /b %ERRORLEVEL%
