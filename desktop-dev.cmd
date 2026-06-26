@echo off
setlocal

rem ============================================================================
rem  AgentMux desktop dev launcher
rem
rem  Starts the Tauri desktop app in dev mode (npm run tauri:dev). If cargo is
rem  not already on PATH, it injects the repo-vendored Rust toolchain under
rem  .toolchains\cargo so `tauri dev` can run `cargo metadata`. Mirrors the env
rem  setup in tools\run-desktop-tauri-build.ps1.
rem
rem  Usage: double-click this file, or run `desktop-dev` from any shell.
rem ============================================================================

rem Repo root = the folder this script lives in (strip trailing backslash).
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"

set "LOCAL_CARGO_HOME=%ROOT%\.toolchains\cargo"
set "LOCAL_RUSTUP_HOME=%ROOT%\.toolchains\rustup"
set "LOCAL_CARGO_BIN=%LOCAL_CARGO_HOME%\bin"

where cargo >nul 2>nul
if errorlevel 1 (
  if exist "%LOCAL_CARGO_BIN%\cargo.exe" (
    set "CARGO_HOME=%LOCAL_CARGO_HOME%"
    set "RUSTUP_HOME=%LOCAL_RUSTUP_HOME%"
    if not defined RUSTUP_TOOLCHAIN set "RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc"
    set "PATH=%LOCAL_CARGO_BIN%;%PATH%"
    echo [desktop-dev] using vendored cargo: %LOCAL_CARGO_BIN%
  ) else (
    echo [desktop-dev] ERROR: cargo is not on PATH and no vendored toolchain
    echo [desktop-dev]        was found at %LOCAL_CARGO_BIN%.
    echo [desktop-dev]        Install Rust, or run:
    echo [desktop-dev]          powershell -NoProfile -ExecutionPolicy Bypass -File tools\bootstrap-windows.ps1
    exit /b 1
  )
) else (
  echo [desktop-dev] using cargo already on PATH
)

pushd "%ROOT%"
echo [desktop-dev] starting: npm --prefix apps/desktop run tauri:dev
call npm --prefix apps/desktop run tauri:dev
set "EXITCODE=%ERRORLEVEL%"
popd

exit /b %EXITCODE%
