@echo off
rem Builds Forma3D.exe on Windows. Double-click this file, or run it from a terminal.
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
cargo build --release
if errorlevel 1 (
    echo.
    echo The build failed. See the messages above.
    pause
    exit /b 1
)

if not exist dist mkdir dist
copy /y target\release\forma3d.exe dist\Forma3D.exe >nul
echo.
echo Done: %cd%\dist\Forma3D.exe
echo It runs on its own; copy it anywhere. Settings are kept in %%APPDATA%%\forma3d.
pause
