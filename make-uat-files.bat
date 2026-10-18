@echo off
setlocal EnableExtensions
title Excel Formula Inspector - Generate UAT workbooks

REM ===========================================================================
REM  Generates the UAT test workbooks (Excel files) for manual acceptance tests.
REM
REM  Output folder (default):  <repo>\uat-fixtures
REM  Each file is created from scratch by code, so it contains no real data.
REM  The script also runs the real parser and rule engine over every file and
REM  prints the expected findings, which are documented in docs\UAT-GUIDE.md
REM
REM  Requirements:
REM    - Rust 1.77 or newer (rustup default toolchain)
REM    - Visual Studio Build Tools with the "Desktop development with C++"
REM      workload (provides the MSVC linker)
REM
REM  Usage:
REM    make-uat-files.bat                 Write to <repo>\uat-fixtures
REM    make-uat-files.bat D:\UAT\files     Write to a chosen folder
REM ===========================================================================

cd /d "%~dp0"

set "PAUSE_AT_END=1"
set "OUTPUT_ARG="
if /I "%~1"=="--no-pause" set "PAUSE_AT_END=0"
if not "%~1"=="" if /I not "%~1"=="--no-pause" set "OUTPUT_ARG=%~1"

echo(
echo ===========================================================================
echo  Excel Formula Inspector - Generate UAT workbooks
echo  Folder: %CD%
echo ===========================================================================
echo(

echo [1/2] Checking prerequisites...
where cargo >nul 2>nul || goto :missing_cargo
for /f "delims=" %%v in ('rustc --version 2^>nul') do set "RUST_VERSION=%%v"
echo       Rust : %RUST_VERSION%
echo       OK

echo(
echo [2/2] Generating workbooks and expected results...
echo       Running: cargo run --example make_uat_fixtures
echo       The first run compiles the generator, so it may take a minute.
echo(

if defined OUTPUT_ARG (
    call cargo run --manifest-path "src-tauri\Cargo.toml" --example make_uat_fixtures -- "%OUTPUT_ARG%"
) else (
    call cargo run --manifest-path "src-tauri\Cargo.toml" --example make_uat_fixtures
)
if errorlevel 1 goto :failed

echo(
echo ===========================================================================
echo  DONE
echo  Open docs\UAT-GUIDE.md for the checklist and the expected results.
echo ===========================================================================
if "%PAUSE_AT_END%"=="1" (
    echo(
    pause
)
endlocal
exit /b 0

:missing_cargo
echo(
echo [ERROR] Rust (cargo) was not found in PATH.
echo         Install it with rustup: https://rustup.rs/
echo         Then open a new terminal so PATH is refreshed.
goto :abort

:failed
echo(
echo ===========================================================================
echo  FAILED - see the messages above
echo ===========================================================================
echo  Hint: linker errors (LNK1104, "link.exe not found") mean the Visual Studio
echo        Build Tools with the "Desktop development with C++" workload are
echo        missing.
goto :abort

:abort
if "%PAUSE_AT_END%"=="1" (
    echo(
    pause
)
endlocal
exit /b 1
