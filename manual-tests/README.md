# Manual tests

Five PowerShell scripts that check the governed-healing work against a real, running backend: real HTTP calls, and real Chromium for some of them. They are run by hand; CI does not run them.

They were written for pull requests #40, #41, #42 and the crawl-evidence bridge (#43) while those were still separate branches. They have not been re-run on master since those merged.

| Script | What it checks | Needs |
|---|---|---|
| `manual-test-1-autonomous-healing.ps1` | #41: a recurring failure raises one healing proposal by itself, with the real selector; no second one while it is open; a proposal that changes nothing gets a 409 on approve and on execute; the `AIQA_AUTONOMOUS_HEALING=off` kill switch (`-ExpectDisabled`) | the backend |
| `manual-test-2-persistence.ps1` | #42: a proposal keeps its executor, environment, task name, target, timeout and parameters; a production proposal's healing run is held for approval; `-RealBrowser` also runs an approved playwright proposal in Chromium; `-OldProject <id>` checks a project whose proposals predate the fix | the backend (Chromium for `-RealBrowser`) |
| `manual-test-3-schema-path.ps1` | #40: the planning endpoint's `schema_file_path` is confined to `AIQA_SCHEMA_ROOT`. Run it twice, as its header describes (root unset, then set, with `-Configured`) | the backend |
| `manual-test-4-bridge.ps1` | #43: with a crawl on record, a recurring failure on `#delete-user-btn` is diagnosed from the crawl and the proposal suggests `#delete-user-account-btn`, held for a person | the backend, the Playwright browsers, `python` on PATH (it serves a small page on port 8090) |
| `manual-test-5-real-browser.ps1` | #41's original bug, with real Chromium: five real Playwright runs of one task (fail, pass, pass, pass, fail) raise exactly one proposal on their own, with the selector read from the failing run's own output. Takes 1-2 minutes | the backend, the Playwright browsers |

Scripts 2, 3 and 5 were written to fail on the code from before the fix they cover.

## Running one

1. Use a checkout whose data you do not mind changing, and a fresh database, so that the backend prints a bootstrap admin API key. In that checkout:

   ```powershell
   Get-ChildItem infra\data\orchestrator.sqlite3* | Rename-Item -NewName { $_.Name + ".bak" }
   cd services\orchestrator
   cargo run -p aiqa-orchestrator     # prints: AIQA bootstrap admin API key ... aiqa_xxxx
   ```

2. In a second terminal, from the repo root:

   ```powershell
   .\manual-tests\manual-test-1-autonomous-healing.ps1 -Key aiqa_xxxx
   ```

3. Each check prints `PASS` or `FAIL`, and the last line says `ALL CHECKS PASSED` or `N CHECK(S) FAILED`. Stop the backend with Ctrl+C (or `stop.bat`) when you are done.

Things to know:

- The backend listens on port 8080, so stop any other instance first.
- Scripts 4 and 5 need the repo root: run them from it (the current directory is the default) or pass `-RepoRoot`.
- A database that newer code has migrated cannot be opened by older code (`migration 18 was previously applied but is missing`). Use a fresh database when you switch between old and new code.
