# MANUAL TEST 1 - autonomous healing (PR #41)
#   checks: a recurring failure raises ONE proposal on its own (with the real selector), no pile-up while it is
#   open, a proposal that changes nothing cannot be approved or executed (409), and the kill switch.
#   Kill switch: stop the backend, run  $env:AIQA_AUTONOMOUS_HEALING = "off"  in the SAME terminal, start it again,
#   then run this script with -ExpectDisabled  (expect: 0 proposals).
#
# HOW TO RUN: see README.md in this folder (fresh database, backend on port 8080, run from the repo root).
param(
    [Parameter(Mandatory = $true)][string]$Key,
    [string]$Base = "http://127.0.0.1:8080/api/v1",
    [switch]$ExpectDisabled
)
$ErrorActionPreference = "Stop"
$script:failures = 0

function Call($Method, $Path, $Body = $null) {
    $req = @{
        Method          = $Method
        Uri             = "$Base$Path"
        Headers         = @{ Authorization = "Bearer $Key" }
        ContentType     = "application/json"
        UseBasicParsing = $true
    }
    if ($null -ne $Body) { $req.Body = ($Body | ConvertTo-Json -Depth 10) }
    try {
        $r = Invoke-WebRequest @req
        $parsed = $null
        if ($r.Content) { $parsed = $r.Content | ConvertFrom-Json }
        return [pscustomobject]@{ Status = [int]$r.StatusCode; Body = $parsed; Raw = $r.Content }
    }
    catch {
        $code = 0
        $text = "$($_.Exception.Message)"
        if ($_.Exception.Response) {
            $code = [int]$_.Exception.Response.StatusCode
            if ($_.ErrorDetails -and $_.ErrorDetails.Message) { $text = $_.ErrorDetails.Message }
        }
        return [pscustomobject]@{ Status = $code; Body = $null; Raw = $text }
    }
}

function Check($ok, $what) {
    if ($ok) { Write-Host "  PASS  $what" -ForegroundColor Green }
    else { Write-Host "  FAIL  $what" -ForegroundColor Red; $script:failures++ }
}

function Proposals($proj) { @((Call GET "/projects/$proj/healing/proposals").Body | Where-Object { $_ }) }

# One run of a single simulated task named "login-e2e". The task's params carry the selector it
# "failed on" - the override a hand-built plan can use; real browser runs supply it from their output.
function RunTask($proj, $outcome) {
    $plan = @{
        name        = "manual-run"
        concurrency = 1
        tasks       = @(@{
                id       = "login-e2e"
                name     = "login-e2e"
                simulate = @{ outcome = $outcome; delay_ms = 0 }
                params   = @{ selector = "#login-btn" }
            })
    }
    $r = Call POST "/projects/$proj/executions" $plan
    if ($r.Status -ne 201) { throw "creating a run failed: $($r.Status) $($r.Raw)" }
    $runId = $r.Body.run_id
    for ($i = 0; $i -lt 100; $i++) {
        $d = Call GET "/projects/$proj/executions/$runId"
        if ($d.Body.state -notin @("CREATED", "SCHEDULED", "RUNNING", "PENDING_APPROVAL")) { break }
        Start-Sleep -Milliseconds 100
    }
}

$health = Call GET "/projects"
if ($health.Status -ne 200) { throw "backend not reachable / key rejected: $($health.Status) $($health.Raw)" }

$proj = (Call POST "/projects" @{ name = "manual-phase10-$(Get-Random)" }).Body.id
Write-Host "`nproject: $proj"

Write-Host "`n[1] the same task fails, passes, passes, passes, fails (a recurring, flaky failure)"
foreach ($o in @("fail", "pass", "pass", "pass", "fail")) { RunTask $proj $o }
Start-Sleep -Seconds 2
$props = @(Proposals $proj)

if ($ExpectDisabled) {
    Check ($props.Count -eq 0) "AIQA_AUTONOMOUS_HEALING=off: no proposal was raised (found $($props.Count))"
    Write-Host ""
    if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
    return
}

Check ($props.Count -eq 1) "exactly one proposal was raised on its own (found $($props.Count))"
if ($props.Count -ge 1) {
    $p = $props[0]
    Check ($p.diagnosis_category -eq "RecurringFlakyFailure") "category is RecurringFlakyFailure (got $($p.diagnosis_category))"
    Check ($p.state -eq "PENDING_APPROVAL") "it is held for a person: PENDING_APPROVAL (got $($p.state))"
    Check ($p.confidence -eq 0.3) "confidence is capped at 0.30 (got $($p.confidence))"
    Check ($p.original_actions[0].selector -eq "#login-btn") "it names the selector the task failed on (#login-btn)"
    $same = (($p.original_actions | ConvertTo-Json -Compress) -eq ($p.proposed_actions | ConvertTo-Json -Compress))
    Check $same "it proposes no change (advisory only)"

    Write-Host "`n[2] approving a proposal that changes nothing must be refused"
    $a = Call POST "/projects/$proj/healing/proposals/$($p.id)/approve"
    Check ($a.Status -eq 409) "approve -> 409 (got $($a.Status))"
    Check ("$($a.Raw)" -match "changes nothing") "message says the proposal changes nothing"
    Write-Host "      $($a.Raw)"
}

Write-Host "`n[3] the task keeps failing while that proposal is still open: no pile-up"
RunTask $proj "pass"
RunTask $proj "fail"
Start-Sleep -Seconds 2
$props = @(Proposals $proj)
Check ($props.Count -eq 1) "still exactly one proposal for the task (found $($props.Count))"

Write-Host "`n[4] a no-change proposal that reached APPROVED without a person must not execute"
$noop = @{
    project_id        = $proj
    run_id            = [guid]::NewGuid().ToString()
    task_id           = "noop-task"
    task_name         = "noop"
    executor          = "simulated"
    environment       = "test"
    target_url        = "http://127.0.0.1:8080/app"
    failed_selector   = "#x"
    failure_reason    = "manual test"
    original_actions  = @(@{ type = "CLICK"; selector = "#x" })
    proposed_actions  = @(@{ type = "CLICK"; selector = "#x" })
    diagnosis_category = "Manual"
    confidence        = 0.95
    rationale         = "manual test"
    auto_approve_safe = $true
}
$c = Call POST "/projects/$proj/healing/proposals" $noop
Check ($c.Status -eq 201 -and $c.Body.state -eq "APPROVED") "precondition: it was created already APPROVED (got $($c.Status) $($c.Body.state))"
if ($c.Body) {
    $e = Call POST "/projects/$proj/healing/proposals/$($c.Body.id)/execute"
    Check ($e.Status -eq 409) "execute -> 409 (got $($e.Status))"
    Check ("$($e.Raw)" -match "changes nothing") "message says the proposal changes nothing"
}

Write-Host "`n[5] control: a proposal that really changes a selector can still be approved"
$real = @{
    project_id        = $proj
    run_id            = [guid]::NewGuid().ToString()
    task_id           = "real-task"
    task_name         = "real"
    executor          = "simulated"
    environment       = "test"
    target_url        = "http://127.0.0.1:8080/app"
    failed_selector   = "#login-btn"
    failure_reason    = "manual test"
    original_actions  = @(@{ type = "NAVIGATE"; url = "http://127.0.0.1:8080/app" }, @{ type = "CLICK"; selector = "#login-btn" })
    proposed_actions  = @(@{ type = "NAVIGATE"; url = "http://127.0.0.1:8080/app" }, @{ type = "CLICK"; selector = "[data-testid='login']" })
    diagnosis_category = "SelectorRenamedOrRelocated"
    confidence        = 0.9
    rationale         = "manual test"
}
$r1 = Call POST "/projects/$proj/healing/proposals" $real
Check ($r1.Status -eq 201 -and $r1.Body.state -eq "PENDING_APPROVAL") "created and held for approval (got $($r1.Status) $($r1.Body.state))"
if ($r1.Body) {
    $ok = Call POST "/projects/$proj/healing/proposals/$($r1.Body.id)/approve"
    Check ($ok.Status -eq 200 -and $ok.Body.state -eq "APPROVED") "approve -> 200 APPROVED (got $($ok.Status) $($ok.Body.state))"
}

Write-Host ""
if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
