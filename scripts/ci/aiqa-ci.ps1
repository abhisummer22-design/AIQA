<#
.SYNOPSIS
  AIQA in a CI job (PowerShell): start a run, wait for it to end, save its result as JUnit XML and exit with what it came to.

.DESCRIPTION
  aiqa-ci.ps1 regression <schedule-id>                 repeat the approved test cases of a scheduled regression
  aiqa-ci.ps1 openapi <spec-url-or-file> [base-url]    test an API from its OpenAPI document (by rule, no AI)

  Written for Windows PowerShell 5.1 and PowerShell 7 (tried on 5.1). It is the same as aiqa-ci.sh, and is set the same way, in the environment
  (a key is never an argument, so it is not in the job's command line):

    AIQA_URL              where AIQA runs, e.g. https://aiqa.example.com                          (required)
    AIQA_KEY              an API key that may run tests in the project: the operator role         (required)
    AIQA_PROJECT          the project's id                                                        (required)
    AIQA_FAIL_ON          failing (default): fail the build when a case fails
                          regressed: fail only when a case that passed last time fails now (regression only)
                          never: never fail the build on results
    AIQA_JUNIT_FILE       where to save the JUnit XML                                             (default aiqa-junit.xml)
    AIQA_TIMEOUT_SECONDS  how long to wait for the run to end                                     (default 900)
    AIQA_OPENAPI_AUTH     openapi only: Header=env:VARIABLE, a credential AIQA reads from ITS environment (never a value)
    AIQA_OPENAPI_UNSAFE   openapi only: true to also run the requests that change data (POST, PUT, PATCH, DELETE)

  Exit code:  0 the result is acceptable   1 the build should fail on the result
              2 there is no result: the run could not be started or read, did not end in time, or nothing ran
#>
param(
    [Parameter(Position = 0)] [string] $Mode,
    [Parameter(Position = 1)] [string] $Target,
    [Parameter(Position = 2)] [string] $BaseUrl
)
$ErrorActionPreference = 'Stop'
# Whatever goes wrong that this script did not think of is "no result" (2), never the build failing on a result (1).
trap { [Console]::Error.WriteLine("aiqa-ci: $($_.Exception.Message)"); exit 2 }
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

function Say([string] $Text) { Write-Host "aiqa-ci: $Text" }
function Fail([string] $Text) { [Console]::Error.WriteLine("aiqa-ci: $Text"); exit 2 }
function Usage {
    Get-Help -Full $PSCommandPath | Out-String | ForEach-Object { [Console]::Error.WriteLine($_) }
    exit 2
}

if ($Mode -eq 'regression') { if (-not $Target -or $BaseUrl) { Usage } }
elseif ($Mode -eq 'openapi') { if (-not $Target) { Usage } }
else { Usage }

if (-not $env:AIQA_URL) { Fail 'AIQA_URL is not set (where AIQA runs, e.g. https://aiqa.example.com)' }
if (-not $env:AIQA_KEY) { Fail 'AIQA_KEY is not set (an API key with the operator role, from a secret)' }
if (-not $env:AIQA_PROJECT) { Fail 'AIQA_PROJECT is not set (the project''s id)' }
$root = $env:AIQA_URL.TrimEnd('/')
$failOn = if ($env:AIQA_FAIL_ON) { $env:AIQA_FAIL_ON } else { 'failing' }
if ($failOn -notin 'failing', 'regressed', 'never') { Fail 'AIQA_FAIL_ON must be failing, regressed or never' }
if ($Mode -eq 'openapi' -and $failOn -eq 'regressed') { Fail 'AIQA_FAIL_ON=regressed compares with the last run, which only a regression has: use failing' }
$timeout = 900
if ($env:AIQA_TIMEOUT_SECONDS) {
    if ($env:AIQA_TIMEOUT_SECONDS -notmatch '^[0-9]+$') { Fail 'AIQA_TIMEOUT_SECONDS must be a number of seconds' }
    $timeout = [int] $env:AIQA_TIMEOUT_SECONDS
}
$junitFile = if ($env:AIQA_JUNIT_FILE) { $env:AIQA_JUNIT_FILE } else { 'aiqa-junit.xml' }
if ($env:AIQA_OPENAPI_AUTH -and $env:AIQA_OPENAPI_AUTH -notmatch '^[^=]+=env:.+$') { Fail 'AIQA_OPENAPI_AUTH must look like Header=env:VARIABLE' }

# The status, the answer and its headers; a failure to get an answer at all has status 0 and says why in Error.
function Invoke-Aiqa([string] $Method, [string] $Url, [string] $Body, [int] $Seconds) {
    $call = @{ Uri = $Url; Method = $Method; Headers = @{ Authorization = "Bearer $($env:AIQA_KEY)" }; TimeoutSec = $Seconds; UseBasicParsing = $true }
    if ($Body) { $call.Body = [Text.Encoding]::UTF8.GetBytes($Body); $call.ContentType = 'application/json' }
    try {
        $response = Invoke-WebRequest @call
        return [pscustomobject]@{ Status = [int] $response.StatusCode; Body = [string] $response.Content; Headers = $response.Headers; Error = '' }
    }
    catch {
        $failed = $_.Exception.Response
        if ($null -eq $failed) { return [pscustomobject]@{ Status = 0; Body = ''; Headers = @{}; Error = $_.Exception.Message } }
        $text = "$($_.ErrorDetails.Message)"
        if (-not $text -and $failed -is [Net.HttpWebResponse]) { $text = (New-Object IO.StreamReader($failed.GetResponseStream())).ReadToEnd() }
        return [pscustomobject]@{ Status = [int] $failed.StatusCode; Body = $text; Headers = @{}; Error = '' }
    }
}

function Header-Of($Answer, [string] $Name) {
    try { $value = $Answer.Headers[$Name]; if ($value -is [array]) { $value = $value[0] }; "$value" } catch { '' }
}

# What went wrong, in the words the answer gave.
function Problem($Answer) {
    $message = if ($Answer.Body -match '"message"\s*:\s*"([^"]*)"') { $Matches[1] } else { $Answer.Error }
    "HTTP $($Answer.Status)" + $(if ($message) { ": $message" } else { '' })
}

if ($Mode -eq 'regression') {
    $startUrl = "$root/api/v1/projects/$($env:AIQA_PROJECT)/regression/schedules/$Target/run"
    $reportUrl = "$root/api/v1/projects/$($env:AIQA_PROJECT)/regression/runs"
    $request = ''
}
else {
    $startUrl = "$root/api/v1/projects/$($env:AIQA_PROJECT)/openapi/runs"
    $reportUrl = $startUrl
    $body = [ordered]@{}
    if (Test-Path -LiteralPath $Target -PathType Leaf) { $body.spec = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $Target).Path) } else { $body.spec_url = $Target }
    if ($BaseUrl) { $body.base_url = $BaseUrl }
    if ($env:AIQA_OPENAPI_UNSAFE -eq 'true') { $body.include_unsafe = $true }
    if ($env:AIQA_OPENAPI_AUTH) { $name, $value = $env:AIQA_OPENAPI_AUTH -split '=', 2; $body.auth_headers = @{ $name = $value } }
    $request = $body | ConvertTo-Json -Compress -Depth 5
}

$started = Invoke-Aiqa 'POST' $startUrl $request 120
if ($started.Status -ne 201) { Fail "could not start the run: $(Problem $started)" }
$runId = "$(($started.Body | ConvertFrom-Json).id)"
if (-not $runId) { Fail 'the answer to starting the run has no id' }
Say "started the $Mode run $runId"

# Ask for the report, waiting up to a minute each time, until the run has ended or the time is up. A failed read is tried again twice.
$deadline = (Get-Date).AddSeconds($timeout)
$failedReads = 0
while ($true) {
    $left = [int] ($deadline - (Get-Date)).TotalSeconds
    if ($left -le 0) { Say "the run has not ended after ${timeout}s"; exit 2 }
    $forSeconds = [Math]::Min($left, 60)
    $report = Invoke-Aiqa 'GET' "$reportUrl/${runId}?format=junit&wait=$forSeconds" '' ($forSeconds + 30)
    if ($report.Status -eq 200) {
        $failedReads = 0
        if ((Header-Of $report 'X-AIQA-Final') -eq 'true') { break }
        Start-Sleep -Seconds 1
    }
    else {
        $failedReads++
        if (($report.Status -ne 0 -and $report.Status -lt 500) -or $failedReads -gt 2) { Fail "could not read the run: $(Problem $report)" }
        Start-Sleep -Seconds 5
    }
}

$verdict = Header-Of $report 'X-AIQA-Verdict'
$junitPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($junitFile)
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $junitPath) | Out-Null
[IO.File]::WriteAllText($junitPath, $report.Body, (New-Object Text.UTF8Encoding($false)))
Say "the run ended: $verdict. JUnit XML is in $junitFile"

# The report in words, for the job's log and, on GitHub, for the job's summary page.
$words = Invoke-Aiqa 'GET' "$reportUrl/${runId}?format=markdown" '' 60
if ($words.Status -eq 200) {
    Write-Host $words.Body
    if ($env:GITHUB_STEP_SUMMARY) { Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $words.Body -Encoding utf8 }
}

switch ($verdict) {
    'PASSING' { $code = 0 }
    'REGRESSED' { $code = if ($failOn -eq 'never') { 0 } else { 1 } }
    'FAILING' { $code = if ($failOn -eq 'failing') { 1 } else { 0 } }
    'NOTHING_RAN' { Say 'nothing ran, so nothing was tested'; $code = 2 }
    default { Say "the run ended with a verdict this script does not know: '$verdict'"; $code = 2 }
}
if ($code -eq 0) { Say 'the build may go on' }
elseif ($code -eq 1) { Say "the build fails on this result (AIQA_FAIL_ON=$failOn)" }
exit $code
