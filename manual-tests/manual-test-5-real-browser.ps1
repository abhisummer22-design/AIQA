# MANUAL TEST 5 - the ORIGINAL bug, with real Chromium (PR #41)
#   Five REAL Playwright runs of one task (fail, pass, pass, pass, fail). Nothing names the selector up front: the engine
#   must read it from the failing run's own output (the AIQA_HEALING_EVENT line) and raise ONE proposal by itself.
#   Takes ~1-2 minutes (launches Chromium 5 times). RUN IT FROM THE REPO ROOT (or pass -RepoRoot).
#   On the original version of PR #41 (commit 9ef4908) this finds 0 proposals - it could never fire on a real run.
#
# HOW TO RUN: see README.md in this folder (fresh database, backend on port 8080, run from the repo root).
param([Parameter(Mandatory = $true)][string]$Key, [string]$Base = "http://127.0.0.1:8080/api/v1",
      [string]$RepoRoot = (Get-Location).Path)
$ErrorActionPreference = "Stop"
$script:failures = 0
function Call($Method, $Path, $Body = $null) {
    $req = @{ Method = $Method; Uri = "$Base$Path"; Headers = @{ Authorization = "Bearer $Key" }; ContentType = "application/json"; UseBasicParsing = $true }
    if ($null -ne $Body) { $req.Body = ($Body | ConvertTo-Json -Depth 10) }
    try {
        $r = Invoke-WebRequest @req
        $parsed = $null; if ($r.Content) { $parsed = $r.Content | ConvertFrom-Json }
        return [pscustomobject]@{ Status = [int]$r.StatusCode; Body = $parsed; Raw = $r.Content }
    }
    catch {
        $code = 0; $text = "$($_.Exception.Message)"
        if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode; if ($_.ErrorDetails -and $_.ErrorDetails.Message) { $text = $_.ErrorDetails.Message } }
        return [pscustomobject]@{ Status = $code; Body = $null; Raw = $text }
    }
}
function Check($ok, $what) { if ($ok) { Write-Host "  PASS  $what" -ForegroundColor Green } else { Write-Host "  FAIL  $what" -ForegroundColor Red; $script:failures++ } }

$gen = Join-Path $RepoRoot "executors\playwright\tests\generated"
New-Item -ItemType Directory -Force -Path $gen | Out-Null
$noBom = New-Object System.Text.UTF8Encoding($false)
$page = "await page.setContent('<html><body><button id=""present"">Present</button></body></html>');"
$failSpec = @"
import { test } from "@playwright/test";
import { healingLocator } from "../../helpers/healing";
test("manual flaky - fails", async ({ page }) => {
  $page
  await (await healingLocator(page, "#login-btn-that-does-not-exist", 1, "CLICK")).click();
});
"@
$passSpec = @"
import { test } from "@playwright/test";
import { healingLocator } from "../../helpers/healing";
test("manual flaky - passes", async ({ page }) => {
  $page
  await (await healingLocator(page, "#present", 1, "CLICK")).click();
});
"@
[System.IO.File]::WriteAllText((Join-Path $gen "manual-flaky-fail.spec.ts"), $failSpec, $noBom)
[System.IO.File]::WriteAllText((Join-Path $gen "manual-flaky-pass.spec.ts"), $passSpec, $noBom)

try {
    $proj = (Call POST "/projects" @{ name = "manual-real-output-$(Get-Random)" }).Body.id
    Write-Host "project: $proj"
    Write-Host "`nfive REAL browser runs of one task (fail, pass, pass, pass, fail) - nothing names the selector up front"
    foreach ($o in @("fail", "pass", "pass", "pass", "fail")) {
        $spec = "tests/generated/manual-flaky-$o.spec.ts"
        $plan = @{ name = "manual-real"; concurrency = 1; tasks = @(@{ id = "login-e2e"; name = "login-e2e"; executor = "playwright"; params = @{ spec = $spec } }) }
        $r = Call POST "/projects/$proj/executions" $plan
        if ($r.Status -ne 201) { throw "creating a run failed: $($r.Status) $($r.Raw)" }
        $state = ""
        for ($i = 0; $i -lt 240; $i++) { $state = (Call GET "/projects/$proj/executions/$($r.Body.run_id)").Body.state; if ($state -notin @("CREATED", "SCHEDULED", "RUNNING", "PENDING_APPROVAL")) { break }; Start-Sleep -Milliseconds 500 }
        Write-Host ("  run {0,-4} -> {1}" -f $o, $state)
    }
    Start-Sleep -Seconds 3
    $props = @((Call GET "/projects/$proj/healing/proposals").Body | Where-Object { $_ })
    Check ($props.Count -eq 1) "exactly one proposal was raised on its own (found $($props.Count))"
    if ($props.Count -ge 1) {
        $p = $props[0]
        Check ($p.original_actions[0].selector -eq "#login-btn-that-does-not-exist") "the selector came from the browser run's own output (got $($p.original_actions[0].selector))"
        Check ($p.diagnosis_category -eq "RecurringFlakyFailure") "category RecurringFlakyFailure (got $($p.diagnosis_category))"
        Check ($p.state -eq "PENDING_APPROVAL") "held for a person (got $($p.state))"
    }
}
finally {
    Remove-Item -ErrorAction SilentlyContinue (Join-Path $gen "manual-flaky-fail.spec.ts"), (Join-Path $gen "manual-flaky-pass.spec.ts")
}
Write-Host ""
if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
