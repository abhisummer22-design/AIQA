# MANUAL TEST 3 - planning endpoint schema_file_path (security fix in PR #40)
#   Run it TWICE, restarting the backend in between:
#     A) AIQA_SCHEMA_ROOT NOT set:   .\manual-tests\manual-test-3-schema-path.ps1 -Key <key>
#     B) same terminal:   $env:AIQA_SCHEMA_ROOT = "$env:TEMP\aiqa-schema-root" ; start the backend ;
#                         .\manual-tests\manual-test-3-schema-path.ps1 -Key <key> -Configured
#   The script creates its own fixture files under %TEMP%. Before the fix, run A FAILS: a file outside any root is
#   read and a plan is built from it (HTTP 200).
#
# HOW TO RUN: see README.md in this folder (fresh database, backend on port 8080, run from the repo root).
param(
    [Parameter(Mandatory = $true)][string]$Key,
    [string]$Base = "http://127.0.0.1:8080/api/v1",
    [switch]$Configured,
    [string]$Root = "$env:TEMP\aiqa-schema-root"
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
        $parsed = $null
        try { $parsed = $text | ConvertFrom-Json } catch {}
        return [pscustomobject]@{ Status = $code; Body = $parsed; Raw = $text }
    }
}

function Check($ok, $what) {
    if ($ok) { Write-Host "  PASS  $what" -ForegroundColor Green }
    else { Write-Host "  FAIL  $what" -ForegroundColor Red; $script:failures++ }
}

$openapi = '{"openapi":"3.0.3","info":{"title":"Widget API","version":"1.0.0"},"paths":{"/widgets":{"get":{"operationId":"listWidgets","responses":{"200":{"description":"ok"}}}}}}'

# fixtures
$Root = [System.IO.Path]::GetFullPath($Root)
$evil = "$Root-evil"                      # shares the root's name as a string prefix, is NOT inside it
$outside = Join-Path $env:TEMP "aiqa-outside-host-secret.json"
New-Item -ItemType Directory -Force -Path $Root, $evil | Out-Null
# UTF-8 WITHOUT a BOM: Set-Content -Encoding UTF8 (Windows PowerShell 5.1) writes one, and the schema
# inspector cannot parse a BOM-prefixed file (the request then fails with a 500).
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText((Join-Path $Root "widgets.json"), $openapi, $utf8NoBom)
[System.IO.File]::WriteAllText((Join-Path $evil "widgets.json"), $openapi, $utf8NoBom)
[System.IO.File]::WriteAllText($outside, $openapi, $utf8NoBom)

$health = Call GET "/projects"
if ($health.Status -ne 200) { throw "backend not reachable / key rejected: $($health.Status) $($health.Raw)" }
$proj = (Call POST "/projects" @{ name = "manual-schema-$(Get-Random)" }).Body.id
$url = "/projects/$proj/planning/test-plan"

function Plan($extra) {
    $body = @{ project_id = $proj }
    foreach ($k in $extra.Keys) { $body[$k] = $extra[$k] }
    return (Call POST $url $body)
}
function Msg($r) { if ($r.Body -and $r.Body.error) { $r.Body.error.message } else { "$($r.Raw)" } }

Write-Host "`n[1] an inline schema (schema_content) works in every configuration"
$r = Plan @{ schema_content = $openapi }
Check ($r.Status -eq 200) "inline schema -> 200 (got $($r.Status))"
Check (@($r.Body.api_areas_considered) -contains "/widgets") "the plan was built from the inline schema (/widgets)"

# every request below names a real, valid schema file somewhere on this machine
$refusals = @{}
function Refused($label, $path) {
    $r = Plan @{ schema_file_path = $path }
    Check ($r.Status -eq 400) "$label -> 400 (got $($r.Status))"
    $m = Msg $r
    Check ($m -notmatch "aiqa-outside|aiqa-schema-root|win\.ini|\\Users\\") "$label -> the error does not echo any path"
    $script:refusals[$label] = $m
}

if (-not $Configured) {
    Write-Host "`n[2] no AIQA_SCHEMA_ROOT: server-side schema files are switched off"
    Refused "outside file (absolute path)" $outside
    Refused "C:\Windows\win.ini" "C:\Windows\win.ini"
    Refused "UNC path" "\\localhost\C$\Windows\win.ini"
    $m = $script:refusals["outside file (absolute path)"]
    Check ($m -match "schema_content" -and $m -match "AIQA_SCHEMA_ROOT") "the message tells the caller what to do (schema_content / AIQA_SCHEMA_ROOT)"
}
else {
    Write-Host "`n[2] AIQA_SCHEMA_ROOT = $Root : a file INSIDE it is allowed"
    $a = Plan @{ schema_file_path = (Join-Path $Root "widgets.json") }
    Check ($a.Status -eq 200) "absolute path inside the root -> 200 (got $($a.Status) $(if ($a.Status -ne 200) { Msg $a }))"
    Check (@($a.Body.api_areas_considered) -contains "/widgets") "the plan was built from that file"
    $b = Plan @{ schema_file_path = "widgets.json" }
    Check ($b.Status -eq 200) "relative name (relative to the root) -> 200 (got $($b.Status) $(if ($b.Status -ne 200) { Msg $b }))"

    Write-Host "`n[3] everything else is refused with ONE identical message"
    Refused "outside file (absolute path)" $outside
    Refused "..\ traversal out of the root" "..\aiqa-outside-host-secret.json"
    Refused "sibling folder sharing the root's name prefix" (Join-Path $evil "widgets.json")
    Refused "file that does not exist" (Join-Path $Root "no-such-file.json")
    Refused "a folder instead of a file" $Root
    Refused "UNC path" "\\localhost\C$\Windows\win.ini"
    $distinct = @($script:refusals.Values | Sort-Object -Unique)
    Check ($distinct.Count -eq 1) "all six refusals read exactly the same (so the endpoint can't be used to probe which files exist)"
    Write-Host "      message: $($distinct[0])"
}

Write-Host ""
if ($script:failures -eq 0) { Write-Host "ALL CHECKS PASSED" -ForegroundColor Green } else { Write-Host "$($script:failures) CHECK(S) FAILED" -ForegroundColor Red }
