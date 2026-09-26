#!/usr/bin/env bash
# AIQA in a CI job: start a run, wait for it to end, save its result as JUnit XML and exit with what it came to.
#
#   aiqa-ci.sh regression <schedule-id>                 repeat the approved test cases of a scheduled regression
#   aiqa-ci.sh openapi <spec-url-or-file> [base-url]    test an API from its OpenAPI document (by rule, no AI)
#
# It needs only bash, curl, sed, grep and awk. Everything else is set in the environment (a key is never an argument, so it is not in
# the job's command line):
#
#   AIQA_URL              where AIQA runs, e.g. https://aiqa.example.com                          (required)
#   AIQA_KEY              an API key that may run tests in the project: the operator role         (required)
#   AIQA_PROJECT          the project's id                                                        (required)
#   AIQA_FAIL_ON          failing (default): fail the build when a case fails
#                         regressed: fail only when a case that passed last time fails now (regression only)
#                         never: never fail the build on results
#   AIQA_JUNIT_FILE       where to save the JUnit XML                                             (default aiqa-junit.xml)
#   AIQA_TIMEOUT_SECONDS  how long to wait for the run to end                                     (default 900)
#   AIQA_OPENAPI_AUTH     openapi only: Header=env:VARIABLE, a credential AIQA reads from ITS environment (never a value)
#   AIQA_OPENAPI_UNSAFE   openapi only: true to also run the requests that change data (POST, PUT, PATCH, DELETE)
#
# Exit code:  0 the result is acceptable   1 the build should fail on the result
#             2 there is no result: the run could not be started or read, did not end in time, or nothing ran

set -u

say() { printf 'aiqa-ci: %s\n' "$*"; }
fail() { printf 'aiqa-ci: %s\n' "$*" >&2; exit 2; }
usage() { sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }

mode=${1:-}
case "$mode" in
  regression) [ $# -eq 2 ] || usage ;;
  openapi) { [ $# -ge 2 ] && [ $# -le 3 ]; } || usage ;;
  *) usage ;;
esac

[ -n "${AIQA_URL:-}" ] || fail "AIQA_URL is not set (where AIQA runs, e.g. https://aiqa.example.com)"
[ -n "${AIQA_KEY:-}" ] || fail "AIQA_KEY is not set (an API key with the operator role, from a secret)"
[ -n "${AIQA_PROJECT:-}" ] || fail "AIQA_PROJECT is not set (the project's id)"
base_url=${AIQA_URL%/}
fail_on=${AIQA_FAIL_ON:-failing}
case "$fail_on" in
  failing | regressed | never) ;;
  *) fail "AIQA_FAIL_ON must be failing, regressed or never" ;;
esac
if [ "$mode" = openapi ] && [ "$fail_on" = regressed ]; then
  fail "AIQA_FAIL_ON=regressed compares with the last run, which only a regression has: use failing"
fi
timeout=${AIQA_TIMEOUT_SECONDS:-900}
case "$timeout" in
  '' | *[!0-9]*) fail "AIQA_TIMEOUT_SECONDS must be a number of seconds" ;;
esac
junit_file=${AIQA_JUNIT_FILE:-aiqa-junit.xml}
if [ -n "${AIQA_OPENAPI_AUTH:-}" ]; then
  case "$AIQA_OPENAPI_AUTH" in
    ?*=env:?*) ;;
    *) fail "AIQA_OPENAPI_AUTH must look like Header=env:VARIABLE" ;;
  esac
fi

work=$(mktemp -d 2>/dev/null || mktemp -d -t aiqa) || fail "cannot make a temporary directory"
trap 'rm -rf "$work"' EXIT
# The key goes to curl in a file only this user can read, not on its command line.
(umask 077 && printf 'header = "Authorization: Bearer %s"\n' "$AIQA_KEY" > "$work/curl.cfg") || fail "cannot write in the temporary directory"

# call METHOD URL BODY-FILE-OR-EMPTY MAX-SECONDS: the HTTP status is left in $status, the answer in $work/body, its headers in $work/headers
call() {
  : > "$work/body"
  : > "$work/headers"
  set -- -sS -K "$work/curl.cfg" -X "$1" --max-time "$4" -D "$work/headers" -o "$work/body" -w '%{http_code}' "$2" ${3:+-H 'Content-Type: application/json' --data-binary "@$3"}
  status=$(curl "$@" 2> "$work/curl-error") || status=000
}

# The last header of that name in the answer.
header_of() {
  tr -d '\r' < "$work/headers" | grep -i "^$1:" | tail -1 | sed 's/^[^:]*:[[:space:]]*//'
}

# What went wrong, in the words the answer or curl gave.
problem() {
  message=$(grep -o '"message" *: *"[^"]*"' "$work/body" | head -1 | sed 's/^"message" *: *"//; s/"$//')
  [ -n "$message" ] || message=$(tr -d '\r\n' < "$work/curl-error" | cut -c1-200)
  printf 'HTTP %s%s' "$status" "${message:+: $message}"
}

json_string() { printf '"%s"' "$(printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g')"; }

# A file's text as a JSON string: quotes, backslashes, tabs and line ends escaped.
json_file() {
  tab=$(printf '\t')
  printf '"'
  tr -d '\r' < "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e "s/$tab/\\\\t/g" | awk '{ printf "%s\\n", $0 }'
  printf '"'
}

case "$mode" in
  regression)
    start_url="$base_url/api/v1/projects/$AIQA_PROJECT/regression/schedules/$2/run"
    report_url="$base_url/api/v1/projects/$AIQA_PROJECT/regression/runs"
    request=""
    ;;
  openapi)
    start_url="$base_url/api/v1/projects/$AIQA_PROJECT/openapi/runs"
    report_url="$start_url"
    request="$work/request.json"
    {
      printf '{'
      if [ -f "$2" ]; then
        printf '"spec":'
        json_file "$2"
      else
        printf '"spec_url":%s' "$(json_string "$2")"
      fi
      [ -z "${3:-}" ] || printf ',"base_url":%s' "$(json_string "$3")"
      [ "${AIQA_OPENAPI_UNSAFE:-false}" != true ] || printf ',"include_unsafe":true'
      if [ -n "${AIQA_OPENAPI_AUTH:-}" ]; then
        printf ',"auth_headers":{%s:%s}' "$(json_string "${AIQA_OPENAPI_AUTH%%=*}")" "$(json_string "${AIQA_OPENAPI_AUTH#*=}")"
      fi
      printf '}'
    } > "$request"
    ;;
esac

call POST "$start_url" "$request" 120
[ "$status" = 201 ] || fail "could not start the run: $(problem)"
run_id=$(grep -o '"id" *: *"[^"]*"' "$work/body" | head -1 | sed 's/^"id" *: *"//; s/"$//')
[ -n "$run_id" ] || fail "the answer to starting the run has no id"
say "started the $mode run $run_id"

# Ask for the report, waiting up to a minute each time, until the run has ended or the time is up. A failed read is tried again twice.
deadline=$(( $(date +%s) + timeout ))
failed_reads=0
while :; do
  left=$(( deadline - $(date +%s) ))
  [ "$left" -gt 0 ] || { say "the run has not ended after ${timeout}s"; exit 2; }
  for_seconds=$(( left < 60 ? left : 60 ))
  call GET "$report_url/$run_id?format=junit&wait=$for_seconds" "" $(( for_seconds + 30 ))
  if [ "$status" = 200 ]; then
    failed_reads=0
    [ "$(header_of x-aiqa-final)" != true ] || break
    sleep 1
  else
    failed_reads=$(( failed_reads + 1 ))
    case "$status" in
      000 | 5??) [ "$failed_reads" -le 2 ] || fail "could not read the run: $(problem)" ;;
      *) fail "could not read the run: $(problem)" ;;
    esac
    sleep 5
  fi
done

verdict=$(header_of x-aiqa-verdict)
mkdir -p "$(dirname "$junit_file")" 2> /dev/null
cp "$work/body" "$junit_file" || fail "cannot save $junit_file"
say "the run ended: $verdict. JUnit XML is in $junit_file"

# The report in words, for the job's log and, on GitHub, for the job's summary page.
call GET "$report_url/$run_id?format=markdown" "" 60
if [ "$status" = 200 ]; then
  cat "$work/body"
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then cat "$work/body" >> "$GITHUB_STEP_SUMMARY"; fi
fi

case "$verdict" in
  PASSING) code=0 ;;
  REGRESSED) if [ "$fail_on" = never ]; then code=0; else code=1; fi ;;
  FAILING) if [ "$fail_on" = failing ]; then code=1; else code=0; fi ;;
  NOTHING_RAN) say "nothing ran, so nothing was tested"; code=2 ;;
  *) say "the run ended with a verdict this script does not know: '$verdict'"; code=2 ;;
esac
case "$code" in
  0) say "the build may go on" ;;
  1) say "the build fails on this result (AIQA_FAIL_ON=$fail_on)" ;;
esac
exit "$code"
