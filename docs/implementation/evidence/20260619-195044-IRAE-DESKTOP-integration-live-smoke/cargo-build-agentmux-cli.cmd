@echo off
cd /d D:\Workspace\irae\agentmux
D:\Workspace\irae\agentmux\.toolchains\cargo\bin\cargo.exe build -p agentmux-cli > D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\cargo-build-agentmux-cli.stdout.txt 2> D:\Workspace\irae\agentmux\docs\implementation\evidence\20260619-195044-IRAE-DESKTOP-integration-live-smoke\cargo-build-agentmux-cli.stderr.txt
exit /b %ERRORLEVEL%
