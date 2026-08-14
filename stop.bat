@echo off
REM Stops both AIQA processes.
REM
REM Ports changed when the bundled Express server was removed: the UI used to
REM be served by that server on :3000, and is now served by Vite on :5173.
REM :3000 is still swept here so a stale pre-cleanup process gets cleaned up.

echo Stopping AIQA...

for %%P in (5173 8080 3000) do (
    for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":%%P " ^| findstr "LISTENING"') do (
        echo   port %%P - killing PID %%a
        taskkill /F /PID %%a >nul 2>&1
    )
)

echo Done.
