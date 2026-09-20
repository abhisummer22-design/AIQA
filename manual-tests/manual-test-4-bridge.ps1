# MANUAL TEST 4 - crawl-evidence bridge (PR #43)
#   checks: with a crawl on record, a recurring failure on #delete-user-btn is diagnosed by the real SelfHealingAgent from
#   the crawl and the proposal SUGGESTS #delete-user-account-btn (held for a person). Needs: python on PATH (serves a
#   tiny page on port 8090) and the Playwright browsers installed. RUN IT FROM THE REPO ROOT (or pass -RepoRoot).
#   Why the crawler is run directly: the crawl API refuses loopback targets other than port 8080 (SSRF guard), so a
#   local page cannot be crawled through the API; this script runs the REAL crawler (helpers/crawler.ts) on the page and
#   registers the result as the project's crawl, which is what the API would have left behind.
#
# HOW TO RUN: see README.md in this folder (fresh database, backend on port 8080, run from the repo root).
param([Parameter(Mandatory = $true)][string]$Key, [string]$Base = "http://127.0.0.1:8080/api/v1", [int]$Port = 8090, [string]$RepoRoot = (Get-Location).Path)
$ErrorActionPreference = "Stop"
$script:failures = 0
$repo = $RepoRoot

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
function RunTask($proj, $outcome) {
    $plan = @{ name = "manual-run"; concurrency = 1; tasks = @(@{ id = "delete-user-e2e"; name = "delete-user-e2e"; simulate = @{ outcome = $outcome; delay_ms = 0 }; params = @{ selector = "#delete-user-btn" } }) }
    $r = Call POST "/projects/$proj/executions" $plan
    if ($r.Status -ne 201) { throw "creating a run failed: $($r.Status) $($r.Raw)" }
    for ($i = 0; $i -lt 100; $i++) { $d = Call GET "/projects/$proj/executions/$($r.Body.run_id)"; if ($d.Body.state -notin @("CREATED", "SCHEDULED", "RUNNING", "PENDING_APPROVAL")) { break }; Start-Sleep -Milliseconds 100 }
}

$noBom = New-Object System.Text.UTF8Encoding($false)
$pageDir = Join-Path $env:TEMP "aiqa-crawl-page"
New-Item -ItemType Directory -Force -Path $pageDir | Out-Null
[System.IO.File]::WriteAllText((Join-Path $pageDir "index.html"), '<!doctype html><html><head><title>Users</title></head><body><h1>User Management</h1><button id="delete-user-account-btn">Delete User Account</button></body></html>', $noBom)
$srv = Start-Process -FilePath python -ArgumentList "-m", "http.server", "$Port", "--bind", "127.0.0.1" -WorkingDirectory $pageDir -PassThru -WindowStyle Hidden
$id = [guid]::NewGuid().ToString()
$gen = Join-Path $repo "executors\playwright\tests\generated"
$specName = "crawl-$id.spec.ts"
try {
    Start-Sleep -Seconds 2
    $spec = @"
import { test } from "@playwright/test";
import { runCrawl, CrawlPlan } from "../../helpers/crawler";
const PLAN: CrawlPlan = {"baseUrl":"http://127.0.0.1:$Port/","roles":[{"name":"anonymous","loginActions":[]}]};
test("crawl $id", async ({ browser }) => {
  await runCrawl(browser, PLAN, "crawl-result-$id.json");
});
"@
    [System.IO.File]::WriteAllText((Join-Path $gen $specName), $spec, $noBom)

    Write-Host "`n[1] run the REAL crawler (helpers/crawler.ts) on the local page"
    Push-Location (Join-Path $repo "executors\playwright")
    $out = cmd.exe /c "npx playwright test tests/generated/$specName --reporter=line 2>&1" | Out-String
    Pop-Location
    Write-Host ($out.Trim() -split "`n" | Select-Object -Last 4 | ForEach-Object { "      $_" } | Out-String).TrimEnd()
    $resultFile = Join-Path $gen "crawl-result-$id.json"
    Check (Test-Path $resultFile) "the crawler wrote its result file"
    $result = Get-Content $resultFile -Raw | ConvertFrom-Json
    $hints = @($result.roles.pages.elements | ForEach-Object { $_.selectorHint })
    Write-Host "      crawled elements: $($hints -join ', ')"
    Check ($hints -contains "#delete-user-account-btn") "real crawl output contains #delete-user-account-btn"

    Write-Host "`n[2] register that crawl as a completed 'Crawl: ...' run of a new project, then a recurring failure"
    $proj = (Call POST "/projects" @{ name = "manual-bridge-real-$(Get-Random)" }).Body.id
    $reg = Call POST "/projects/$proj/executions" @{ name = "Crawl: http://127.0.0.1:$Port/"; concurrency = 1; tasks = @(@{ id = $id; name = "Crawl: http://127.0.0.1:$Port/"; simulate = @{ outcome = "pass"; delay_ms = 0 } }) }
    Check ($reg.Status -eq 201) "crawl run registered (got $($reg.Status))"
    Start-Sleep -Seconds 1
    foreach ($o in @("fail", "pass", "pass", "pass", "fail")) { RunTask $proj $o }
    Start-Sleep -Seconds 3
    $props = @((Call GET "/projects/$proj/healing/proposals").Body | Where-Object { $_ })
    Check ($props.Count -eq 1) "exactly one proposal (found $($props.Count))"
    if ($props.Count -ge 1) {
        $p = $props[0]
        Check ($p.diagnosis_category -eq "SelectorRenamedOrRelocated") "diagnosed from the real crawl: SelectorRenamedOrRelocated (got $($p.diagnosis_category))"
        Check ($p.original_actions[0].selector -eq "#delete-user-btn") "original: #delete-user-btn (got $($p.original_actions[0].selector))"
        Check ($p.proposed_actions[0].selector -eq "#delete-user-account-btn") "suggested: #delete-user-account-btn (got $($p.proposed_actions[0].selector))"
        Check ($p.state -eq "PENDING_APPROVAL") "held for a person (got $($p.state))"
        Write-Host "      confidence: $($p.confidence)   rationale: $($p.rationale)"
    }
}
finally {
    if ($srv) { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
    Remove-Item -ErrorAction SilentlyContinue (Join-Path $gen $specName), (Join-Path $gen "crawl-result-$id.json")
}
Write-Host ""
if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
