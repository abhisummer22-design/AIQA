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
    REM This was setting every variable in .env to an EMPTY value, so no API key
    REM ever reached the orchestrator and every provider reported NOT_CONFIGURED.
    REM Two separate faults, both needed:
    REM
    REM   1. `tokens=1,*` does not do what it reads like. In cmd, the comma means
    REM      "token 1, then the REST as token 2" -- but with `delims==` the whole
    REM      `KEY=VALUE` line is a single token, so `%%A` never equals the bare
    REM      variable name and `%%B` is always empty. `tokens=1,2*` is the form
    REM      that actually yields name in %%A and value in %%B.
    REM   2. `%AIQA_ENVFILE%` was a `%`-variable, which cmd expands BEFORE the
    REM      `for /f` body runs -- inside a parenthesised block it saw nothing, and
    REM      `type` failed on a blank path. `!AIQA_ENVFILE!` (delayed expansion) is
    REM      the correct form for this scope.
    REM
    REM Lines with an empty value are skipped: `set "KEY="` is a no-op here, and
    REM leaving it alone is what lets GROQ_REQUEST_TOKEN_LIMIT and friends stay
    REM unset so the code's own defaults apply.
    setlocal enabledelayedexpansion
    for /f "usebackq tokens=1,2* delims==" %%A in (`type "!AIQA_ENVFILE!" ^| findstr /b /i /v "REM #"`) do (
        if not "%%B"=="" set "%%A=%%B"
    )
    endlocal
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
