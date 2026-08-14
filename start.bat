@echo off
REM AIQA local startup.
REM
REM Two processes are required, and the UI is useless without both:
REM   1. the Rust orchestrator on :8080  - the actual backend
REM   2. the Vite dev server on :5173    - the UI, which proxies /api to :8080
REM
REM The UI has no data of its own. If the orchestrator is not running it will
REM say "Orchestrator unreachable" rather than showing placeholder numbers.

echo ========================================
echo   AIQA - starting orchestrator + web UI
echo ========================================
echo.

echo [1/2] Starting Rust orchestrator on http://127.0.0.1:8080 ...
start "AIQA Orchestrator" cmd /k "cd /d "%~dp0services\orchestrator" && cargo run -p aiqa-orchestrator"

REM Give the orchestrator a head start so the UI's first health check succeeds.
timeout /t 3 /nobreak >nul

echo [2/2] Starting web UI on http://localhost:5173 ...
start "AIQA Web UI" cmd /k "cd /d "%~dp0apps\web" && npm run dev"

echo.
echo ========================================
echo   Web UI:       http://localhost:5173
echo   Orchestrator: http://127.0.0.1:8080
echo ========================================
echo.
echo FIRST RUN: the orchestrator prints a bootstrap admin API key to its
echo console exactly once. Copy it, open the UI, and paste it into
echo Settings -^> Orchestrator API key. Without a key every authenticated
echo screen will correctly report "Authentication required".
echo.
echo Stop both with stop.bat
echo.
