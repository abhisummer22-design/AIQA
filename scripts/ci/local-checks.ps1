<#
.SYNOPSIS
  Runs on this machine the checks GitHub CI runs (.github/workflows/ci.yml): the secret scan, the frontend and the backend.

.DESCRIPTION
  scripts/ci/local-checks.ps1 [-Only secrets|frontend|backend] [-Install] [-TestThreads N] [-Range <git range>] [-Full] [-DryRun]

    secrets   Gitleaks over the commits that are not on origin/master yet (-Range), or over the whole history (-Full), and over
              what is staged. Needs the gitleaks program on PATH (or its path in GITLEAKS_PATH):
              https://github.com/gitleaks/gitleaks/releases (check the zip against the release's checksums file).
    frontend  forentend: npm run lint (tsc), npm test (vitest), npm run build. npm install runs first when node_modules is
              missing, or when -Install is given.
    backend   services/orchestrator: cargo clippy --all-targets, then cargo test.

  The steps are independent, as CI's jobs are: each runs even when another failed, and the last lines say how each came out.
  Within a step the first failing command stops it, as in CI. CI's "cargo fmt --all" only reformats and cannot fail, so it is
  not repeated. -TestThreads (default 6) limits how many backend tests run at once: a few older timing-based tests can fail
  when many run together on a fast machine, and pass when run alone. So when backend tests fail (up to 10 of them), they are run
  once more, alone: if they then pass the step is FLAKY (it passes, and says which tests needed that), if not it is FAIL.

  Exit code:  0 every step that ran passed   1 a step failed   2 a step could not run (a tool is missing)
#>
param(
    [ValidateSet('all', 'secrets', 'frontend', 'backend')] [string] $Only = 'all',
    [switch] $Install,
    [ValidateRange(1, 64)] [int] $TestThreads = 6,
    [string] $Range = 'origin/master..HEAD',
    [switch] $Full,
    [switch] $DryRun
)
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$results = New-Object System.Collections.ArrayList

function Test-Wanted([string] $Name) { $Only -eq 'all' -or $Only -eq $Name }

function Add-Result([string] $Name, [string] $Status, [string] $Note, $Watch) {
    [void] $results.Add([pscustomobject]@{ Name = $Name; Status = $Status; Note = $Note; Seconds = [int] $Watch.Elapsed.TotalSeconds })
}

# Runs one command in a folder and says whether it exited with 0. With -DryRun it only says what it would run.
function Invoke-Step([string] $Dir, [string] $Label, [scriptblock] $Command) {
    Write-Host "--- $Label   (in $Dir)" -ForegroundColor Cyan
    if ($DryRun) { Write-Host "    would run: $Command"; return $true }
    Push-Location $Dir
    try { & $Command | Out-Host; return ($LASTEXITCODE -eq 0) } finally { Pop-Location }
}

# The result of a step whose tools are there: PASS, FLAKY (passed only after the tests named in $Flaky were run again alone), FAIL, or DRY RUN.
function Add-Outcome([string] $Name, [bool] $Ok, $Watch, [string[]] $Flaky = @()) {
    $status = if ($DryRun) { 'DRY RUN' } elseif (-not $Ok) { 'FAIL' } elseif ($Flaky.Count -gt 0) { 'FLAKY' } else { 'PASS' }
    Add-Result $Name $status $(if ($Flaky.Count -gt 0) { 'passed only when run alone: ' + ($Flaky -join ', ') } else { '' }) $Watch
}

# The names of the tests a cargo test log says failed (the last "failures:" list of it).
function Get-FailedTests([string] $Log) {
    $lines = @(Get-Content -LiteralPath $Log -ErrorAction SilentlyContinue)
    $at = -1
    for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i].Trim() -eq 'failures:') { $at = $i } }
    $names = @()
    if ($at -ge 0) { for ($i = $at + 1; $i -lt $lines.Count -and $lines[$i].Trim() -ne ''; $i++) { $names += $lines[$i].Trim() } }
    return $names
}

# A step whose tool is missing: skipped when everything was asked for, not run when it was the one thing asked for.
function Add-Missing([string] $Name, [string] $Why, $Watch) {
    Write-Host "--- ${Name}: $Why" -ForegroundColor Yellow
    Add-Result $Name $(if ($Only -eq $Name) { 'NOT RUN' } else { 'SKIPPED' }) $Why $Watch
}

# ------------------------------------------------------------------------------------------------------------ secrets
if (Test-Wanted 'secrets') {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $gitleaks = $env:GITLEAKS_PATH
    if (-not $gitleaks) { $gitleaks = Get-Command gitleaks -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source }
    if (-not $gitleaks -and -not $DryRun) {
        Add-Missing 'secrets' 'gitleaks is not installed (see -? for where to get it and how to check it)' $watch
    }
    else {
        if (-not $gitleaks) { $gitleaks = 'gitleaks' }
        $base = @('git', '--no-banner', '--redact=100', '--exit-code', '2')
        $ok = $true
        if ($Full) {
            $ok = Invoke-Step $root 'gitleaks: the whole history' { & $gitleaks @base $root }
        }
        else {
            $count = (git -C $root rev-list --count $Range 2>$null)
            if ($LASTEXITCODE -ne 0) {
                Write-Host "--- gitleaks: the range '$Range' is not known here: git fetch origin, or give -Range or -Full" -ForegroundColor Red
                $ok = $false
            }
            elseif ([int] $count -eq 0) {
                Write-Host "--- gitleaks: no commit in $Range to scan (-Full scans the whole history)"
            }
            else {
                $ok = Invoke-Step $root "gitleaks: the $count commit(s) in $Range" { & $gitleaks @base "--log-opts=$Range" $root }
            }
        }
        if ($ok -and (git -C $root diff --cached --name-only)) {
            $ok = Invoke-Step $root 'gitleaks: what is staged' { & $gitleaks @base --staged $root }
        }
        Add-Outcome 'secrets' $ok $watch
    }
}

# ---------------------------------------------------------------------------------------------------------- frontend
if (Test-Wanted 'frontend') {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $dir = Join-Path $root 'forentend'
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
        Add-Missing 'frontend' 'npm (Node.js) is not installed' $watch
    }
    else {
        $ok = $true
        if ($Install -or -not (Test-Path (Join-Path $dir 'node_modules'))) { $ok = Invoke-Step $dir 'npm install' { npm install } }
        if ($ok) { $ok = Invoke-Step $dir 'npm run lint  (tsc)' { npm run lint } }
        if ($ok) { $ok = Invoke-Step $dir 'npm test  (vitest)' { npm test } }
        if ($ok) { $ok = Invoke-Step $dir 'npm run build' { npm run build } }
        Add-Outcome 'frontend' $ok $watch
    }
}

# ----------------------------------------------------------------------------------------------------------- backend
if (Test-Wanted 'backend') {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $dir = Join-Path $root 'services\orchestrator'
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Add-Missing 'backend' 'cargo (Rust) is not installed' $watch
    }
    else {
        $ok = Invoke-Step $dir 'cargo clippy --all-targets  (warnings do not fail it, as in CI)' { cargo clippy --all-targets }
        $flaky = @()
        if ($ok) {
            $log = Join-Path ([IO.Path]::GetTempPath()) ("local-checks-cargo-test-$PID.log")
            $ok = Invoke-Step $dir "cargo test  (-- --test-threads=$TestThreads)" { cargo test -- "--test-threads=$TestThreads" | Tee-Object -FilePath $log }
            if (-not $ok -and -not $DryRun) {
                $names = @(Get-FailedTests $log)
                if ($names.Count -gt 0 -and $names.Count -le 10) {
                    Write-Host "--- $($names.Count) test(s) failed: run alone once more, as a timing-based test can fail when many run together: $($names -join ', ')" -ForegroundColor Yellow
                    if (Invoke-Step $dir 'cargo test  (the failed test(s), alone)' { cargo test -- --exact --test-threads=1 @names }) { $ok = $true; $flaky = $names }
                }
            }
            Remove-Item -LiteralPath $log -ErrorAction SilentlyContinue
        }
        Add-Outcome 'backend' $ok $watch $flaky
    }
}

# ---------------------------------------------------------------------------------------------------------- summary
Write-Host ''
Write-Host '=== summary ===' -ForegroundColor Cyan
foreach ($r in $results) {
    $color = switch ($r.Status) { 'PASS' { 'Green' } 'FAIL' { 'Red' } 'NOT RUN' { 'Red' } default { 'Yellow' } }
    $line = '{0,-8} {1,-9} {2,4}s  {3}' -f $r.Status, $r.Name, $r.Seconds, $r.Note
    Write-Host $line.TrimEnd() -ForegroundColor $color
}
if ($results | Where-Object { $_.Status -eq 'SKIPPED' }) { Write-Host 'A skipped step was NOT checked: install what it needs, or ask for it alone with -Only.' -ForegroundColor Yellow }
if ($results | Where-Object { $_.Status -eq 'FAIL' }) { exit 1 }
if ($results | Where-Object { $_.Status -eq 'NOT RUN' }) { exit 2 }
exit 0
