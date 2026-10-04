/**
 * Tests for the latency histogram this page now reads.
 *
 * Separate from `TelemetryPage.test.tsx` on purpose: that file covers the page as
 * it was written, including two tests that assert the empty state and the honest
 * disclosure, and overwriting it would have destroyed coverage that still holds.
 * This file covers only what changed -- the histogram, and the copy that used to
 * deny it existed.
 *
 * The invariants are mostly about not overclaiming in the *opposite* direction,
 * which is the easy mistake once the data is there:
 *
 *  - a percentile off a histogram is a range, and is rendered as one;
 *  - the mean is exact, and is the only figure shown as a point value;
 *  - a process that has served a request has latency, so that counts as data for
 *    the "nothing recorded yet" empty state;
 *  - no histogram at all is a different situation from an unobserved one;
 *  - what genuinely cannot be told -- latency split by route, status or project --
 *    stays stated.
 */

import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  formatQuantile,
  formatSeconds,
  parsePrometheusText,
  summariseLatency,
  TelemetryPage,
} from './TelemetryPage';

/** A real scrape, the same shape and numbers the running endpoint returns. */
const SCRAPE = [
  '# HELP aiqa_runs_total Number of runs by state',
  '# TYPE aiqa_runs_total gauge',
  'aiqa_runs_total{state="COMPLETED"} 2',
  'aiqa_runs_total{state="FAILED"} 1',
  'aiqa_api_keys_active 10',
  '# HELP http_request_duration_seconds Latency of HTTP requests served by this process',
  '# TYPE http_request_duration_seconds histogram',
  'http_request_duration_seconds_bucket{le="0.005"} 348',
  'http_request_duration_seconds_bucket{le="0.01"} 349',
  'http_request_duration_seconds_bucket{le="0.025"} 350',
  'http_request_duration_seconds_bucket{le="0.05"} 350',
  'http_request_duration_seconds_bucket{le="0.1"} 351',
  'http_request_duration_seconds_bucket{le="0.25"} 351',
  'http_request_duration_seconds_bucket{le="0.5"} 351',
  'http_request_duration_seconds_bucket{le="1"} 351',
  'http_request_duration_seconds_bucket{le="2.5"} 351',
  'http_request_duration_seconds_bucket{le="5"} 352',
  'http_request_duration_seconds_bucket{le="10"} 353',
  'http_request_duration_seconds_bucket{le="+Inf"} 353',
  'http_request_duration_seconds_sum 14.399815',
  'http_request_duration_seconds_count 353',
].join('\n');

describe('summariseLatency', () => {
  const samples = parsePrometheusText(SCRAPE);

  it('reads the real histogram rather than reporting that none exists', () => {
    const l = summariseLatency(samples);
    expect(l).not.toBeNull();
    expect(l!.count).toBe(353);
    expect(l!.sumSeconds).toBeCloseTo(14.399815, 6);
  });

  it('computes the mean exactly, because _sum/_count is the one exact figure', () => {
    const l = summariseLatency(samples)!;
    expect(l.meanMs).toBeCloseTo((14.399815 / 353) * 1000, 6);
    // ~40ms per request across 353 requests: a plausible figure for a local API.
    expect(l.meanMs!).toBeGreaterThan(30);
    expect(l.meanMs!).toBeLessThan(50);
  });

  it('gives a percentile as the bucket range it falls in, never a point', () => {
    const l = summariseLatency(samples)!;
    // 348 of the 353 observations are already under 5ms, so every percentile in
    // this scrape lands in the small buckets. That is the shape of the data, not a
    // rounding artefact: 50% of 353 is 176.5 and the very first bucket (348) is
    // already past it.
    expect(l.p50!.upperSeconds).toBe(0.005);
    expect(l.p50!.lowerSeconds).toBe(0);
    // 95% of 353 is 335.35 -> still under the first bucket's 348.
    expect(l.p95!.upperSeconds).toBe(0.005);
    // 99% of 353 is 349.47, which the le=0.01 bucket (349) just misses.
    expect(l.p99!.upperSeconds).toBe(0.025);
    expect(l.p99!.lowerSeconds).toBe(0.01);
    for (const q of [l.p50!, l.p95!, l.p99!]) {
      expect(q.lowerSeconds).toBeLessThan(q.upperSeconds as number);
    }
  });

  it('puts a percentile in the right bucket when the tail is slow', () => {
    // The scrape above is almost entirely fast, which would let a broken quantile
    // pass by always returning the first bucket. This one has a real tail.
    const l = summariseLatency(
      parsePrometheusText(
        [
          'http_request_duration_seconds_bucket{le="0.01"} 50',
          'http_request_duration_seconds_bucket{le="0.1"} 80',
          'http_request_duration_seconds_bucket{le="1"} 95',
          'http_request_duration_seconds_bucket{le="5"} 99',
          'http_request_duration_seconds_bucket{le="+Inf"} 100',
          'http_request_duration_seconds_sum 12',
          'http_request_duration_seconds_count 100',
        ].join('\n'),
      ),
    )!;
    expect(l.p50!.upperSeconds).toBe(0.01);  // 50th is 50, which le=0.01 (50) reaches
    expect(l.p50!.lowerSeconds).toBe(0);
    expect(l.p95!.upperSeconds).toBe(1);     // 95th is 95 -> le=1 (95)
    expect(l.p95!.lowerSeconds).toBe(0.1);
    expect(l.p99!.upperSeconds).toBe(5);     // 99th is 99 -> le=5 (99)
    expect(l.p99!.lowerSeconds).toBe(1);
    expect(l.p99!.upperSeconds).not.toBeNull();
  });

  it('sorts the +Inf bucket last, so the count fallback reads the right one', () => {
    // Number('+Inf') is NaN, so a plain numeric sort can put it anywhere. That is
    // invisible while an explicit _count sample exists, and wrong the moment it does
    // not -- the count is then taken from the last bucket, and if that is not the
    // +Inf one every percentile is computed against the wrong denominator.
    const noCount = summariseLatency(
      parsePrometheusText(
        [
          'http_request_duration_seconds_bucket{le="0.01"} 10',
          'http_request_duration_seconds_bucket{le="+Inf"} 42',
          'http_request_duration_seconds_sum 3',
        ].join('\n'),
      ),
    )!;
    expect(noCount.count).toBe(42);
    // 50% of 42 is 21, which only the +Inf bucket reaches -> unbounded, and said so.
    expect(noCount.p50!.upperSeconds).toBeNull();
    expect(noCount.p50!.lowerSeconds).toBe(0.01);
  });

  it('tells a missing histogram apart from an unobserved one', () => {
    const none = summariseLatency(parsePrometheusText('aiqa_api_keys_active 3\n'));
    expect(none).toBeNull();

    const unobserved = summariseLatency(
      parsePrometheusText(
        [
          'http_request_duration_seconds_bucket{le="0.005"} 0',
          'http_request_duration_seconds_bucket{le="+Inf"} 0',
          'http_request_duration_seconds_sum 0',
          'http_request_duration_seconds_count 0',
        ].join('\n'),
      ),
    );
    expect(unobserved).not.toBeNull();
    expect(unobserved!.count).toBe(0);
    expect(unobserved!.meanMs).toBeNull();
    expect(unobserved!.p95).toBeNull();
  });

  it('never invents a percentile from a scrape that has no histogram', () => {
    expect(summariseLatency(parsePrometheusText('aiqa_runs_total{state="X"} 1\n'))).toBeNull();
  });
});

describe('formatting', () => {
  it('renders a bucket range, and says so when the range is unbounded', () => {
    expect(formatQuantile({ lowerSeconds: 0, upperSeconds: 0.025, observations: 350 })).toBe(
      '≤ 25 ms',
    );
    expect(formatQuantile({ lowerSeconds: 0.1, upperSeconds: 0.25, observations: 351 })).toBe(
      '100 ms–250 ms',
    );
    expect(formatQuantile({ lowerSeconds: 2.5, upperSeconds: null, observations: 353 })).toBe(
      '> 2.5 s',
    );
    expect(formatQuantile(null)).toBe('—');
  });

  it('scales seconds to the unit that stays readable', () => {
    expect(formatSeconds(0.005)).toBe('5 ms');
    expect(formatSeconds(0.0004)).toBe('400 µs');
    expect(formatSeconds(2.5)).toBe('2.5 s');
  });
});

describe('TelemetryPage — the real latency', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url.includes('/metrics')) return new Response(SCRAPE, { status: 200 });
        return new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } });
      }),
    );
    localStorage.setItem('aiqa.orchestrator.apiKey', 'aiqa_test_key');
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  const renderPage = () =>
    render(
      <MemoryRouter>
        <TelemetryPage />
      </MemoryRouter>,
    );

  it('shows the real latency instead of denying it exists', async () => {
    renderPage();
    await waitFor(() => expect(screen.getByText(/Request Latency/i)).toBeTruthy());
    expect(screen.getByText(/353 requests/)).toBeTruthy();
    // A percentile on screen as a bucket range, not as a fabricated point value.
    // Two of them: on this scrape 348 of 353 requests are under 5ms, so p50 and p95
    // land in the same bucket and render identically. That is the data, not a bug.
    expect(screen.getAllByText('≤ 5 ms').length).toBe(2);
    // p99 clears the first two buckets, so it is visibly a different figure.
    expect(screen.getByText('10 ms–25 ms')).toBeTruthy();
  });

  it('no longer tells the user there is no latency data', async () => {
    renderPage();
    await waitFor(() => expect(screen.getByText(/Request Latency/i)).toBeTruthy());
    // This is the assertion that matters, because the bug was the copy rather than
    // a missing number: the page used to state a falsehood about a live endpoint.
    expect(screen.queryByText(/no latency\/response-time data exists yet/i)).toBeNull();
    expect(screen.queryByText(/no timing data of any kind/i)).toBeNull();
  });

  it('still admits what a single histogram cannot tell you', async () => {
    renderPage();
    await waitFor(() => expect(screen.getByText(/Request Latency/i)).toBeTruthy());
    // Adding the histogram does not make it per-route. Saying otherwise would be
    // the same overclaiming pointed the other way.
    expect(screen.getByText(/split by route, status code or project/i)).toBeTruthy();
    // And the caveat has to sit next to the number, not in a source comment.
    expect(screen.getByText(/bucket each request fell into/i)).toBeTruthy();
  });

  it('counts a served request as data, so a busy orchestrator is not called empty', async () => {
    // Only the histogram is present here -- no runs, no audit rows, no active keys.
    // Before the change this rendered "No telemetry recorded yet", which is wrong:
    // the process had plainly served 353 requests.
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url.includes('/metrics')) {
          return new Response(
            SCRAPE.split('\n')
              .filter((l) => !l.startsWith('aiqa_runs_total') && !l.startsWith('aiqa_api_keys_active'))
              .join('\n'),
            { status: 200 },
          );
        }
        return new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } });
      }),
    );
    renderPage();
    await waitFor(() => expect(screen.getByText(/Request Latency/i)).toBeTruthy());
    expect(screen.queryByText(/No telemetry recorded yet/i)).toBeNull();
  });
});
