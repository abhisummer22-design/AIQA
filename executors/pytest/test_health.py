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
