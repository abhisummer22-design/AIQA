@echo off
echo Starting AIQA Web Dashboard...
cd /d "%~dp0apps\web"
start "AIQA Web Dashboard" cmd /k "npm run dev"
echo Web Dashboard started in a new window!
