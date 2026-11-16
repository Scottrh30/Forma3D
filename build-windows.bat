@echo off
rem Builds the classic (egui) Forma3D on Windows. For the Tauri app see the README. Double-click this file, or run it from a terminal.
rem Needs Rust (https://rustup.rs): the installer sets up everything else.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust is not installed. Get it from https://rustup.rs , then run this file again.
    echo ^(Choose the default install; it also installs the Visual Studio C++ build tools if needed.^)
    pause
    exit /b 1
)

echo Building Forma3D (release). The first build takes a few minutes...
cargo build --release -p forma3d-classic
if errorlevel 1 (
    echo.
    echo The build failed. See the messages above.
    pause
    exit /b 1
)

if not exist dist mkdir dist
copy /y target\release\forma3d-classic.exe dist\Forma3D-classic.exe >nul
echo.
echo Done: %cd%\dist\Forma3D-classic.exe
echo It runs on its own; copy it anywhere. Settings are kept in %%APPDATA%%\forma3d.
pause
