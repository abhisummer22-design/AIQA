@echo off
setlocal enabledelayedexpansion

REM ========================================================
REM   AIQA - All-In-One Startup Script
REM ========================================================

cd /d "%~dp0"

echo ========================================================
echo   AIQA - Starting Orchestrator + Web UI
echo ========================================================
echo.

REM 1. Clean up stale instances on ports 8080, 3000, 5173
echo [1/5] Checking and clearing old processes on ports 8080, 3000, 5173...
for %%P in (8080 3000 5173) do (
    for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":%%P " ^| findstr "LISTENING" 2^>nul') do (
        echo   - Freeing port %%P, PID %%a
        taskkill /F /PID %%a >nul 2>&1
    )
)

REM 2. Load .env if present
echo [2/5] Loading environment configurations...
set "AIQA_ENVFILE=%~dp0.env"
if exist "%AIQA_ENVFILE%" (
    for /f "usebackq tokens=1,* delims==" %%A in (`type "%AIQA_ENVFILE%" ^| findstr /b /i /v "REM"`) do (
        set "%%A=%%B"
    )
)

REM 3. Check frontend dependencies
echo [3/5] Verifying frontend dependencies...
if not exist "%~dp0forentend\node_modules" (
    echo   - node_modules not found, running npm install in forentend...
    pushd "%~dp0forentend"
    call npm install
    popd
)

REM 4. Start Rust Orchestrator backend
echo [4/5] Starting Rust Orchestrator on http://127.0.0.1:8080 ...
start "AIQA Orchestrator (Backend)" cmd /k "cd /d "%~dp0services\orchestrator" && cargo run -p aiqa-orchestrator"

echo   - Waiting for orchestrator to become healthy...
set ATTEMPTS=0

:wait_orch
set /a ATTEMPTS+=1
ping 127.0.0.1 -n 2 >nul
for /f "tokens=*" %%R in ('curl.exe -s http://127.0.0.1:8080/health 2^>nul') do (
    if "%%R"=="ok" (
        echo   - Orchestrator is READY and HEALTHY!
        goto orch_ready
    )
)
if !ATTEMPTS! lss 30 (
    echo   - Waiting for backend... attempt !ATTEMPTS! of 30
    goto wait_orch
)
echo   [WARNING] Orchestrator took longer than 30s to respond to /health.
echo   Proceeding to start Web UI anyway.

:orch_ready

REM 5. Start Web UI (Vite dev server)
echo [5/5] Starting Web UI on http://localhost:3000 ...
start "AIQA Web UI (Frontend)" cmd /k "cd /d "%~dp0forentend" && npm run dev"

REM Wait 3 seconds for Vite dev server to bind port, then open browser
ping 127.0.0.1 -n 4 >nul
echo.
echo Opening browser: http://localhost:3000 ...
start http://localhost:3000

echo.
echo ========================================================
echo   AIQA IS READY!
echo ========================================================
echo   Web UI:       http://localhost:3000
echo   Orchestrator: http://127.0.0.1:8080
echo ========================================================
echo.
echo NOTE:
echo   - Keep opened console windows running while using AIQA.
echo   - To stop all AIQA services, run: stop.bat
echo.
pause
