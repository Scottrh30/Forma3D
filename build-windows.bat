@echo off
rem Builds the Forma3D app (Tauri + Leptos) on Windows. Double-click this file, or run it from a terminal.
rem Needs Rust (https://rustup.rs) and Node.js (https://nodejs.org); this script sets up the rest.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust is not installed. Get it from https://rustup.rs , then run this file again.
    echo ^(Choose the default install; it also installs the Visual Studio C++ build tools if needed.^)
    pause
    exit /b 1
)
where npx >nul 2>nul
if errorlevel 1 (
    echo Node.js is not installed. Get it from https://nodejs.org , then run this file again.
    pause
    exit /b 1
)

echo Setting up the WebAssembly target and Trunk (only the first time)...
rustup target add wasm32-unknown-unknown
where trunk >nul 2>nul
if errorlevel 1 cargo install trunk

echo Building Forma3D (release). The first build takes a while...
call npx --yes @tauri-apps/cli@2 build
if errorlevel 1 (
    echo.
    echo The build failed. See the messages above.
    pause
    exit /b 1
)

if not exist dist mkdir dist
copy /y target\release\forma3d.exe dist\Forma3D.exe >nul
for %%f in (target\release\bundle\nsis\*.exe) do copy /y "%%f" dist\Forma3D-setup.exe >nul
echo.
echo Done: %cd%\dist\Forma3D.exe and %cd%\dist\Forma3D-setup.exe
pause
