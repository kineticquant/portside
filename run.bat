@echo off
REM Portside all-in-one run - double-click in Explorer and do nothing else.
REM Installs toolchain (one UAC Yes click, enforced by Windows), prompts
REM for WSL2 install if missing (recommended, Docker needs it), starts
REM Docker Desktop and waits for it, npm installs, launches Tauri dev.
REM NEVER reboots on its own and NEVER registers itself to run at startup.
REM If a reboot is needed (WSL / Docker), it just says so - reboot whenever
REM suits you, then double-click run.bat again.
REM check-env exit codes: 0 ready, 1 missing (setup), 2 reboot-would-help.
setlocal
pushd "%~dp0"

powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\check-env.ps1"
set RC=%ERRORLEVEL%
if %RC%==0 goto dock
if %RC%==2 goto rebootlater

powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-windows.ps1"
if errorlevel 1 goto setupfail

powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\check-env.ps1"
set RC=%ERRORLEVEL%
if %RC%==0 goto dock
if %RC%==2 goto rebootlater
echo.
echo SETUP FAILED - still not ready and nothing left to auto-install. See TOOLCHAIN.md.
echo If you cancelled partway (Ctrl+C), just re-run run.bat - it picks up where it left off.
pause
exit /b 1

:setupfail
echo.
echo SETUP FAILED - see output above. Window stays open so you can read it.
pause
exit /b 1

:rebootlater
echo.
echo A reboot would finish setup, but nothing reboots without your say-so.
echo Reboot whenever suits you, then double-click run.bat again.
pause
exit /b 2

:dock
REM --- Docker daemon: auto-start Desktop and wait (up to ~4 min), then go ---
docker info >nul 2>&1
if not errorlevel 1 goto launch
if exist "C:\Program Files\Docker\Docker\Docker Desktop.exe" (
  echo Docker daemon down - starting Docker Desktop minimized to tray...
  start "" /min "C:\Program Files\Docker\Docker\Docker Desktop.exe"
) else (
  echo Docker Desktop not found - installing automatically...
  powershell -NoProfile -ExecutionPolicy Bypass -File "scripts\setup-windows.ps1"
  if errorlevel 1 goto setupfail
)
set WAITED=0
:waitdock
docker info >nul 2>&1
if not errorlevel 1 goto launch
if %WAITED% GEQ 48 goto daemonfail
timeout /t 5 /nobreak >nul
set /a WAITED+=1
goto waitdock

:daemonfail
echo.
echo Docker Desktop did not become ready in ~4 minutes. This usually means
echo WSL2 or a reboot is pending - reboot whenever suits you, then
echo double-click run.bat again. Nothing was scheduled or installed to startup.
pause
exit /b 2

:launch
REM Tauri's resource compiler (rc.exe) lives in the Windows SDK, which is
REM NOT on PATH in a plain terminal - put the newest one there so the
REM Rust build script can find it (cargo check/test never catch this).
REM NOTE: %ProgramFiles(x86)% must NOT appear inside the parens below -
REM its ")" breaks batch block parsing. Copy to KITS first (no parens).
set "KITS=%ProgramFiles(x86)%\Windows Kits\10\bin"
set RCFOUND=
for /f "delims=" %%D in ('dir /b /ad /o-n "%KITS%\10.*" 2^>nul') do (
  if exist "%KITS%\%%D\x64\rc.exe" (
    set "PATH=%KITS%\%%D\x64;%PATH%"
    REM Belt and braces: Tauri resource step reads RC first; registry
    REM auto-discovery fails on some machines even with PATH set.
    set "RC=%KITS%\%%D\x64\rc.exe"
    set RCFOUND=1
    goto rcok
  )
)
:rcok
if not defined RCFOUND echo WARNING: rc.exe not found - Tauri build may fail. See TOOLCHAIN.md.
where rc.exe 2>nul
if not exist "node_modules" (
  echo Installing frontend dependencies...
  call npm install
  if errorlevel 1 (
    echo npm install failed - see output above.
    pause
    exit /b 1
  )
)

echo Starting Portside...
call npm run tauri dev
if errorlevel 1 (
  echo.
  echo Portside exited with an error - see output above.
  pause
  exit /b 1
)
