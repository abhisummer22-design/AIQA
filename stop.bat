@echo off
echo Stopping AIQA Web Dashboard (Port 3000)...
for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":3000" ^| findstr "LISTENING"') do (
    echo Killing PID %%a...
    taskkill /F /PID %%a
)
echo Done.
