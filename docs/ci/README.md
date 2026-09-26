# Using AIQA from a CI job

A CI job can ask AIQA to test something, wait for the answer, put every case on the build's test page (JUnit XML) and fail the build
when a case fails. Two things can be run this way:

- **Scheduled regression** — the cases a project has approved and run once are run again, and the report says what changed since the
  last time (`regression`).
- **API tests from an OpenAPI document** — the cases are made by rule from the document, with no AI (`openapi`).

Nothing in the job needs AI, a browser or a compiler: AIQA runs the tests on its own side, the job only starts the run and reads the result.

## Set it up once

1. **A key for the job.** An admin makes it in AIQA: `POST /admin/api-keys` with `{"name": "ci", "role": "operator", "project_id": "<the project's id>"}`.
   The key is shown once. Keep it as a secret of the CI system, named `AIQA_KEY`. `operator` may start runs and read reports, and only in
   that project.
2. **For a regression, a schedule.** On the *Scheduled Regression* page make a schedule (it may be one that never runs by itself; the job
   runs it when it is asked to). Its page has *Use it from CI*, which gives the schedule's id and the lines to copy.
3. **AIQA must be reachable from the job.** Its address is `AIQA_URL`. A job on a hosted runner cannot reach an AIQA on someone's laptop
   or behind a firewall: use a runner that can (self-hosted), or an address that is open to it.

## The script

`scripts/ci/aiqa-ci.sh` needs only bash, curl, sed, grep and awk. `scripts/ci/aiqa-ci.ps1` does the same in PowerShell (for a Windows
runner). They are set the same way, in the environment:

| Variable | What it is | Default |
|---|---|---|
| `AIQA_URL` | Where AIQA runs, e.g. `https://aiqa.example.com` | required |
| `AIQA_KEY` | The key from step 1 (a secret; never an argument, so it is not in the command line) | required |
| `AIQA_PROJECT` | The project's id | required |
| `AIQA_FAIL_ON` | `failing`: fail the build when a case fails. `regressed`: fail only when a case that passed last time fails now (regression only). `never`: never fail the build on results | `failing` |
| `AIQA_JUNIT_FILE` | Where to save the JUnit XML (its folder is made) | `aiqa-junit.xml` |
| `AIQA_TIMEOUT_SECONDS` | How long to wait for the run to end | `900` |
| `AIQA_OPENAPI_AUTH` | `openapi` only: `Header=env:VARIABLE` — a credential AIQA reads from **its own** environment. A value is never sent | none |
| `AIQA_OPENAPI_UNSAFE` | `openapi` only: `true` also runs the requests that change data (POST, PUT, PATCH, DELETE) | `false` |

```bash
# repeat the approved cases and compare with the last time
AIQA_URL=… AIQA_KEY=… AIQA_PROJECT=… bash scripts/ci/aiqa-ci.sh regression <schedule-id>

# test an API from its OpenAPI document: an address, or a file of the repository
AIQA_URL=… AIQA_KEY=… AIQA_PROJECT=… bash scripts/ci/aiqa-ci.sh openapi https://api.example.com/openapi.json https://api.example.com
AIQA_URL=… AIQA_KEY=… AIQA_PROJECT=… bash scripts/ci/aiqa-ci.sh openapi openapi.yaml https://staging.example.com
```

The script prints the report in words (and adds it to the job's summary page on GitHub), saves the JUnit XML, and exits with:

| Exit code | Meaning |
|---|---|
| `0` | The result is acceptable: everything passed, or what failed is not what `AIQA_FAIL_ON` fails on |
| `1` | The build should fail on the result |
| `2` | There is no result: a variable is missing, the run could not be started or read (the reason is printed), it did not end in time, or nothing ran |

A read that fails for a moment (a connection that drops, a 5xx) is tried again twice; a refusal (401, 403, 404) is not.

## What the routes give, for any other tool

The script only uses these; a job in another language can too.

- Start: `POST /api/v1/projects/:project/regression/schedules/:id/run`, or `POST /api/v1/projects/:project/openapi/runs`. `201` and the run's `id`.
- Read: `GET …/regression/runs/:id` or `GET …/openapi/runs/:id`, with
  - `?format=junit` for JUnit XML (`application/xml`), `?format=markdown` for text, JSON otherwise;
  - `?wait=60` to wait up to that many seconds (at most 300) for the run to end, in the one request. Ask again while it has not ended;
  - the answer's `X-AIQA-Final` (`true` once the run has ended) and `X-AIQA-Verdict` headers, so that nothing has to be parsed.

| Verdict | Means |
|---|---|
| `RUNNING` | The run has not ended |
| `PASSING` | Everything that ran passed |
| `FAILING` | A case failed, or could not be run to the end |
| `REGRESSED` | (regression only) A case that passed last time fails now |
| `NOTHING_RAN` | The run ended and no case ran |

## What the JUnit XML holds

One `<testsuite>`, and a `<testcase>` for each case: a regression's classname is `regression.<schedule name>`; an OpenAPI case's is its
operation (`GET /pets`). A case that failed has `<failure>` with the reason (for an API test: what the document promised and what came
back); one that could not be run to the end has `<error>`; one that was not run (a request that changes data, a case that was not
repeated) is `<skipped>` with the reason. A regression case says in its output what became of it since the last time (`REGRESSED; before: passed (previous run)`).
The suite's properties hold the verdict and the run's id. Times are `0`: the report does not keep how long each case took.

## Examples

- GitHub Actions: [`github-actions.example.yml`](github-actions.example.yml) (a nightly run and one on every pull request; the JUnit file is kept
  as an artifact, and any JUnit reporter action can put it on the checks page).
- GitLab CI:

  ```yaml
  aiqa:
    image: alpine:3
    before_script: [apk add --no-cache bash curl]
    script: [bash scripts/ci/aiqa-ci.sh regression "$AIQA_SCHEDULE_ID"]   # AIQA_URL, AIQA_KEY, AIQA_PROJECT are CI/CD variables
    artifacts:
      when: always
      reports:
        junit: aiqa-junit.xml
  ```

- Jenkins: `withCredentials([string(credentialsId: 'aiqa-key', variable: 'AIQA_KEY')]) { sh 'bash scripts/ci/aiqa-ci.sh regression "$SCHEDULE"' }`, then `junit 'aiqa-junit.xml'` in `post { always { … } }`.
- Azure DevOps: run the script (or the `.ps1` on a Windows agent), then `PublishTestResults@2` with `testResultsFormat: JUnit` and `testResultsFiles: aiqa-junit.xml`, `condition: always()`.

## Good to know

- The report is made from the run as it is when it is asked for; nothing extra is stored. Asking again gives the same answer once the run has ended.
- A regression run started by a job counts as any other run of its schedule: the next run is compared with it.
- A regression is refused (`409`) while the schedule's previous run has not ended, and (`400`) when nothing has been run once yet.
- An OpenAPI run only sends what the document describes, to the address given, and never sends a credential's value to AIQA: the value is
  read from AIQA's environment when a request is made. A private address is refused by AIQA's own guard as for any test.
