@echo off
setlocal EnableExtensions
title Excel Formula Inspector - Build

REM ===========================================================================
REM  Excel Formula Inspector - Windows build script
REM
REM  Produces the release executable and the NSIS installer:
REM    src-tauri\target\release\excel-formula-inspector.exe
REM    src-tauri\target\release\bundle\nsis\*-setup.exe
REM
REM  Requirements:
REM    - Node.js 20 or newer (includes npm)
REM    - Rust 1.77 or newer (rustup default toolchain)
REM    - Visual Studio Build Tools with the "Desktop development with C++"
REM      workload (provides the MSVC linker)
REM    - WebView2 Runtime (preinstalled on most Windows 10/11 systems)
REM    - Internet access on the first run (Tauri downloads the NSIS tooling)
REM
REM  Usage:
REM    build.bat              Build, then wait for a key press
REM    build.bat --no-pause   Build without waiting (CI / automation)
REM ===========================================================================

cd /d "%~dp0"

set "PAUSE_AT_END=1"
if /I "%~1"=="--no-pause" set "PAUSE_AT_END=0"

echo(
echo ===========================================================================
echo  Excel Formula Inspector - Build
echo  Folder: %CD%
echo ===========================================================================
echo(

REM ---------------------------------------------------------------- 1/4 checks
echo [1/4] Checking prerequisites...

where node   >nul 2>nul || goto :missing_node
where npm    >nul 2>nul || goto :missing_npm
where cargo  >nul 2>nul || goto :missing_cargo
where rustc  >nul 2>nul || goto :missing_cargo

for /f "delims=" %%v in ('node --version 2^>nul')  do set "NODE_VERSION=%%v"
for /f "delims=" %%v in ('npm --version 2^>nul')   do set "NPM_VERSION=%%v"
for /f "delims=" %%v in ('rustc --version 2^>nul') do set "RUST_VERSION=%%v"

echo       Node.js : %NODE_VERSION%  (npm %NPM_VERSION%)
echo       Rust    : %RUST_VERSION%
echo       OK

REM ------------------------------------------------------------ 2/4 npm install
echo(
echo [2/4] Front-end dependencies...
if not exist "node_modules" (
    if exist "package-lock.json" (
        echo       node_modules not found. Running: npm ci
        call npm ci
    ) else (
        echo       node_modules not found. Running: npm install
        call npm install
    )
    if errorlevel 1 goto :failed
) else (
    echo       node_modules already present. Skipping.
    echo       Delete the node_modules folder to force a clean install.
)

REM ------------------------------------------------------- 3/4 tauri build
echo(
echo [3/4] Building the release executable...
echo       Running: npm run tauri build
echo       This can take several minutes on the first build.
echo(
call npm run tauri build
if errorlevel 1 goto :failed

REM ------------------------------------------------------------ 4/4 artifacts
echo(
echo [4/4] Build artifacts:

set "EXE_PATH=src-tauri\target\release\excel-formula-inspector.exe"
set "INSTALLER="
for %%f in ("src-tauri\target\release\bundle\nsis\*.exe") do (
    if exist "%%~ff" set "INSTALLER=%%~ff"
)

if exist "%EXE_PATH%" (
    echo       Executable : %CD%\%EXE_PATH%
) else (
    echo       Executable : NOT FOUND - see the build log above
)

if defined INSTALLER (
    echo       Installer  : %INSTALLER%
) else (
    echo       Installer  : NOT FOUND - see the build log above
)

echo(
echo ===========================================================================
echo  BUILD SUCCEEDED
echo ===========================================================================
if "%PAUSE_AT_END%"=="1" (
    echo(
    pause
)
endlocal
exit /b 0

REM ------------------------------------------------------------------ failures
:missing_node
echo(
echo [ERROR] Node.js was not found in PATH.
echo         Install Node.js 20 or newer: https://nodejs.org/
echo         Then open a new terminal so PATH is refreshed.
goto :abort

:missing_npm
echo(
echo [ERROR] npm was not found in PATH.
echo         Reinstall Node.js and make sure "npm" is included.
goto :abort

:missing_cargo
echo(
echo [ERROR] Rust (cargo/rustc) was not found in PATH.
echo         Install it with rustup: https://rustup.rs/
echo         Then open a new terminal so PATH is refreshed.
goto :abort

:failed
echo(
echo ===========================================================================
echo  BUILD FAILED - see the messages above
echo ===========================================================================
echo  Hints:
echo    - Linker errors (LNK1104, "link.exe not found") mean the Visual Studio
echo      Build Tools with the "Desktop development with C++" workload are
echo      missing. Install them and run this script again.
echo    - Compilation errors are listed by cargo above.
echo    - The first build downloads the NSIS tooling, so it needs internet
echo      access. The application itself never uses the network at runtime.
goto :abort

:abort
if "%PAUSE_AT_END%"=="1" (
    echo(
    pause
)
endlocal
exit /b 1
