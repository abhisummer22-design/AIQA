# MANUAL TEST 2 - healing proposal keeps its executor/environment (PR #42)
#   checks: a proposal keeps executor / environment / task name / target / timeout / parameters; a production
#   proposal's healing run is held for approval; an approved playwright proposal really runs in Chromium.
#   -RealBrowser  also runs the Chromium step (~10-40 s).   -OldProject <id>  a project whose proposals predate the fix.
#   Before PR #42 the same script FAILS: proposals come back as simulated/test and the healing run ends INFRA_ERROR
#   (no executor is registered for "simulated").
#
# HOW TO RUN: see README.md in this folder (fresh database, backend on port 8080, run from the repo root).
param(
    [Parameter(Mandatory = $true)][string]$Key,
    [string]$Base = "http://127.0.0.1:8080/api/v1",
    [switch]$RealBrowser,
    [string]$OldProject = ""
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

function Detail($r) { if ($r.Status -ge 200 -and $r.Status -lt 300) { "" } else { $r.Raw } }

function Check($ok, $what) {
    if ($ok) { Write-Host "  PASS  $what" -ForegroundColor Green }
    else { Write-Host "  FAIL  $what" -ForegroundColor Red; $script:failures++ }
}

function Wait-Run($proj, $runId, $seconds) {
    for ($i = 0; $i -lt ($seconds * 2); $i++) {
        $d = Call GET "/projects/$proj/executions/$runId"
        if ($d.Body.state -notin @("CREATED", "SCHEDULED", "RUNNING")) { return $d.Body }
        Start-Sleep -Milliseconds 500
    }
    return (Call GET "/projects/$proj/executions/$runId").Body
}

function NewProposal($proj, $taskId, $executor, $environment, $original, $proposed, $extra = @{}) {
    $body = @{
        project_id         = $proj
        run_id             = [guid]::NewGuid().ToString()
        task_id            = $taskId
        task_name          = "Checkout flow"
        executor           = $executor
        environment        = $environment
        target_url         = "http://127.0.0.1:8080/health"
        failed_selector    = "#pay"
        failure_reason     = "manual test"
        original_actions   = $original
        proposed_actions   = $proposed
        diagnosis_category = "SelectorRenamedOrRelocated"
        confidence         = 0.9
        rationale          = "manual test"
    }
    foreach ($k in $extra.Keys) { $body[$k] = $extra[$k] }
    return (Call POST "/projects/$proj/healing/proposals" $body)
}

$nav = @{ type = "NAVIGATE"; url = "http://127.0.0.1:8080/health" }
$before = @($nav, @{ type = "ASSERT_URL"; url_contains = "health-old" })
$after = @($nav, @{ type = "ASSERT_URL"; url_contains = "zzz-not-in-url" })

$health = Call GET "/projects"
if ($health.Status -ne 200) { throw "backend not reachable / key rejected: $($health.Status) $($health.Raw)" }
$proj = (Call POST "/projects" @{ name = "manual-persist-$(Get-Random)" }).Body.id
Write-Host "`nproject: $proj"

Write-Host "`n[1] a proposal keeps the executor / environment / task name / target it was raised with"
$made = NewProposal $proj "checkout-1" "playwright" "staging" $before $after @{
    timeout_secs          = 45
    requirement_reference = "REQ-7"
    custom_params         = @{ browser = "chromium" }
}
Check ($made.Status -eq 201) "proposal created (got $($made.Status) $(Detail $made))"
if ($made.Body) {
    $got = (Call GET "/projects/$proj/healing/proposals/$($made.Body.id)").Body
    Check ($got.executor -eq "playwright") "executor        = playwright (got $($got.executor))"
    Check ($got.environment -eq "staging") "environment     = staging (got $($got.environment))"
    Check ($got.task_name -eq "Checkout flow") "task_name       = Checkout flow (got $($got.task_name))"
    Check ($got.target_url -eq "http://127.0.0.1:8080/health") "target_url      kept (got $($got.target_url))"
    Check ($got.timeout_secs -eq 45) "timeout_secs    = 45 (got $($got.timeout_secs))"
    Check ($got.requirement_reference -eq "REQ-7") "requirement_reference = REQ-7 (got $($got.requirement_reference))"
    Check ($got.custom_params.browser -eq "chromium") "custom_params.browser = chromium (got $($got.custom_params.browser))"
}

Write-Host "`n[2] a production proposal is executed as production: its run is held for approval"
$prod = NewProposal $proj "checkout-prod" "simulated" "production" $before $after
Check ($prod.Status -eq 201) "production proposal created (got $($prod.Status) $(Detail $prod))"
if ($prod.Body) {
    $null = Call POST "/projects/$proj/healing/proposals/$($prod.Body.id)/approve"
    $ex = Call POST "/projects/$proj/healing/proposals/$($prod.Body.id)/execute"
    Check ($ex.Status -eq 200) "execute -> 200 (got $($ex.Status) $(Detail $ex))"
    if ($ex.Body.healing_run_id) {
        $run = (Call GET "/projects/$proj/executions/$($ex.Body.healing_run_id)").Body
        Check ($run.state -eq "PENDING_APPROVAL") "healing run state = PENDING_APPROVAL (got $($run.state)); old code ran it at once"
    }
}

if ($RealBrowser) {
    Write-Host "`n[3] an approved playwright proposal really runs on playwright (this launches Chromium)"
    $pw = NewProposal $proj "checkout-pw" "playwright" "staging" $before $after
    Check ($pw.Status -eq 201) "playwright proposal created (got $($pw.Status) $(Detail $pw))"
    if ($pw.Body) {
        $null = Call POST "/projects/$proj/healing/proposals/$($pw.Body.id)/approve"
        $ex = Call POST "/projects/$proj/healing/proposals/$($pw.Body.id)/execute"
        Check ($ex.Status -eq 200) "execute -> 200 (got $($ex.Status) $(Detail $ex))"
        if ($ex.Body.healing_run_id) {
            Write-Host "      waiting for the browser run (up to 120 s)..."
            $run = Wait-Run $proj $ex.Body.healing_run_id 120
            Check ($run.state -eq "FAILED") "run FAILED: the URL assertion really ran in a browser and did not match (got $($run.state)); a simulated stand-in would be COMPLETED"
            $ver = Call GET "/projects/$proj/healing/proposals/$($pw.Body.id)/verification"
            Check ($ver.Body.outcome -eq "NOT_VERIFIED" -or $ver.Body.outcome -eq "NotVerified") "verification outcome is not-verified (got $($ver.Body.outcome))"
        }
    }
}
else {
    Write-Host "`n[3] skipped (run with -RealBrowser to launch Chromium)"
}

if ($OldProject) {
    Write-Host "`n[4] proposals stored before the upgrade still load (rebuilt with the old defaults)"
    $old = Call GET "/projects/$OldProject/healing/proposals"
    Check ($old.Status -eq 200) "list -> 200 (got $($old.Status))"
    $oldList = @($old.Body | Where-Object { $_ })
    Check ($oldList.Count -ge 1) "found $($oldList.Count) older proposal(s)"
    if ($oldList.Count -ge 1) {
        Check ($oldList[0].executor -eq "simulated" -and $oldList[0].environment -eq "test") "old row -> executor=simulated, environment=test (got $($oldList[0].executor)/$($oldList[0].environment))"
    }
}

Write-Host ""
if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
