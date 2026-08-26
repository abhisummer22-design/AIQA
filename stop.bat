@echo off
REM Stops all AIQA processes.

echo ========================================================
echo   Stopping AIQA (Backend + Frontend)...
echo ========================================================

for %%P in (8080 3000 5173) do (
    for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":%%P " ^| findstr "LISTENING" 2^>nul') do (
        echo   - Port %%P: terminating PID %%a
        taskkill /F /PID %%a >nul 2>&1
    )
)

echo Done. All AIQA services stopped.
echo ========================================================
