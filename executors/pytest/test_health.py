"""Native API executor fixture (SRD §16 "HTTPX/Pytest is the native API
executor"). Real assertions against the orchestrator's own running server —
no separate product-under-test wired up yet, same approach as the Newman
fixture in ../newman.

Base URL is read from AIQA_TARGET_BASE_URL so the Rust adapter can point this
at whatever's actually running instead of a hardcoded address.
"""

import os

import httpx

BASE_URL = os.environ.get("AIQA_TARGET_BASE_URL", "http://localhost:8080")


def test_health_returns_200_and_ok_body():
    resp = httpx.get(f"{BASE_URL}/health", timeout=5.0)
    assert resp.status_code == 200
    assert resp.text == "ok"


def test_deliberately_wrong_expectation():
    # Kept in the suite on purpose so the demo run has a real (not simulated)
    # pytest failure to show off FRD §2's FAILED status.
    resp = httpx.get(f"{BASE_URL}/health", timeout=5.0)
    assert resp.status_code == 404, "deliberately wrong expectation for the demo"


def test_deliberately_slow_for_live_crash_recovery_check():
    # Not a product assertion. Exists so a live orchestrator-kill-and-restart
    # check (ACC-022/E2E-005 — see AIQA_E2E_WAVE3_EXECUTION_REPORT.md, 23 Aug
    # 2026, for how this was actually used) has a REAL, subprocess-backed
    # task reliably still RUNNING at the moment someone kills the real
    # orchestrator process — deliberately not a SimulatedExecutor delay, so
    # the check also proves whether the real child pytest/python OS process
    # is actually terminated (Windows Job Object containment) when its
    # parent is killed, not just that the in-memory task record gets marked
    # correctly. 15s is enough for a human watching task status to kill into
    # reliably without meaningfully slowing down a casual full-file
    # `pytest` run; the live report used a longer sleep during actual
    # execution to absorb tool round-trip latency, then restored this.
    import time

    time.sleep(15)
    resp = httpx.get(f"{BASE_URL}/health", timeout=5.0)
    assert resp.status_code == 200
