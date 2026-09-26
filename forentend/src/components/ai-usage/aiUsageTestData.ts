/**
 * Phase 8A — a usage report as the backend gives it, for the tests of the AI usage page. Test-only.
 */
import type { AiUsageReport, AiUsageTally } from '../../types/contract';

export const PROJECT_ID = 'proj-a';

export const tally = (over: Partial<AiUsageTally> = {}): AiUsageTally => ({
  calls: 0,
  answered: 0,
  failed: 0,
  prompt_tokens: 0,
  completion_tokens: 0,
  token_calls: 0,
  answered_ms: 0,
  failed_ms: 0,
  avg_answer_ms: null,
  priced_calls: 0,
  no_price_calls: 0,
  no_tokens_calls: 0,
  cost_usd: 0,
  ...over,
});

/** Three days ending on 26 Sep. A hosted provider with a price (groq/llama: 6 calls, one failed, one answered with no token counts) and
 *  a local model with none (ollama/qwen: 6 calls); 12 calls in all, 11 answered, $0.001. */
export function usageReport(over: Partial<AiUsageReport> = {}): AiUsageReport {
  const groq = tally({
    calls: 6,
    answered: 5,
    failed: 1,
    prompt_tokens: 6_500,
    completion_tokens: 1_600,
    token_calls: 4,
    answered_ms: 8_000,
    failed_ms: 30_000,
    avg_answer_ms: 1_600,
    priced_calls: 4,
    no_tokens_calls: 1,
    cost_usd: 0.001,
  });
  return {
    range: { days: 3, from: '2026-09-24', to: '2026-09-26', tz_offset_minutes: 330 },
    totals: tally({
      calls: 12,
      answered: 11,
      failed: 1,
      prompt_tokens: 11_000,
      completion_tokens: 3_600,
      token_calls: 9,
      answered_ms: 59_000,
      failed_ms: 30_000,
      avg_answer_ms: 5_363,
      priced_calls: 4,
      no_price_calls: 6,
      no_tokens_calls: 1,
      cost_usd: 0.001,
    }),
    by_provider: [
      { provider: 'groq', model: 'llama', ...groq },
      {
        provider: 'ollama',
        model: 'qwen',
        ...tally({ calls: 6, answered: 6, prompt_tokens: 4_500, completion_tokens: 2_000, token_calls: 5, answered_ms: 51_000, avg_answer_ms: 8_500, no_price_calls: 6 }),
      },
    ],
    by_task: [
      { task: 'TEST_CASE_GENERATION', ...groq },
      {
        task: 'GENERAL_AI',
        ...tally({ calls: 5, answered: 5, prompt_tokens: 4_000, completion_tokens: 1_800, token_calls: 4, answered_ms: 50_000, avg_answer_ms: 10_000, no_price_calls: 5 }),
      },
      {
        task: 'TEST_CASE_COMPILATION',
        ...tally({ calls: 1, answered: 1, prompt_tokens: 500, completion_tokens: 200, token_calls: 1, answered_ms: 1_000, avg_answer_ms: 1_000, no_price_calls: 1 }),
      },
    ],
    by_day: [
      { day: '2026-09-24', ...tally({ calls: 5, answered: 4, failed: 1, prompt_tokens: 6_000, completion_tokens: 1_500, cost_usd: 0.0009 }) },
      { day: '2026-09-25', ...tally({ calls: 2, answered: 2, prompt_tokens: 1_000, completion_tokens: 300, cost_usd: 0.0001 }) },
      { day: '2026-09-26', ...tally({ calls: 5, answered: 5, prompt_tokens: 4_000, completion_tokens: 1_800 }) },
    ],
    notes: [
      '6 calls went to models with no price (a local model, or a hosted one whose price is not entered in Settings), so no cost is counted for them.',
      '1 call to a provider with a price reported no token counts, so no cost is counted for it.',
      '1 call did not answer; nothing is counted as its cost.',
    ],
    ...over,
  };
}

/** The same with every project at once: split by project, one of the calls made for none. */
export function everywhereReport(): AiUsageReport {
  return usageReport({
    by_project: [
      { project_id: 'proj-a', ...tally({ calls: 6, answered: 5, failed: 1, prompt_tokens: 6_500, completion_tokens: 1_600, token_calls: 4, answered_ms: 8_000, avg_answer_ms: 1_600, priced_calls: 4, no_tokens_calls: 1, cost_usd: 0.001 }) },
      { project_id: 'proj-b', ...tally({ calls: 5, answered: 5, prompt_tokens: 4_000, completion_tokens: 1_800, token_calls: 4, answered_ms: 50_000, avg_answer_ms: 10_000, no_price_calls: 5 }) },
      { project_id: null, ...tally({ calls: 1, answered: 1, prompt_tokens: 500, completion_tokens: 200, token_calls: 1, answered_ms: 1_000, avg_answer_ms: 1_000, no_price_calls: 1 }) },
    ],
  });
}
