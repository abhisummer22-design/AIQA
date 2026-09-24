import React, { useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TaskRoutingChains } from './TaskRoutingChains';
import type { AiModelDefinition, AiProviderInfo, TaskProviderPreference } from '../../types/contract';

const model = (provider_id: string, id: string, text_generation: 'STRONG' | 'NONE' = 'STRONG'): AiModelDefinition => ({
  id,
  provider_id,
  display_name: id,
  family: null,
  parameter_size: null,
  context_window: null,
  capabilities: {
    text_generation,
    structured_output: 'BASIC',
    reasoning: 'BASIC',
    vision: 'NONE',
    embedding: 'NONE',
    tool_use: 'UNKNOWN',
    long_context: 'UNKNOWN',
  },
  status: 'AVAILABLE',
});

const OLLAMA: AiProviderInfo = {
  id: 'ollama',
  name: 'Ollama',
  provider_type: 'LOCAL',
  endpoint: 'http://localhost:11434',
  configured: true,
  reachable: true,
  status: 'HEALTHY',
  models: [model('ollama', 'qwen3:14b'), model('ollama', 'gemma3:12b'), model('ollama', 'nomic-embed-text', 'NONE')],
  pricing: { kind: 'UNMETERED' },
};

const GEMINI: AiProviderInfo = {
  id: 'gemini',
  name: 'Gemini',
  provider_type: 'API',
  endpoint: null,
  configured: true,
  reachable: true,
  status: 'HEALTHY',
  models: [model('gemini', 'gemini-2.5-flash')],
  pricing: { kind: 'METERED', input_usd_per_1k: 0.0003, output_usd_per_1k: 0.0025 },
};

/** Only an embedding model: it can never serve a text task, so this provider has nothing to offer. */
const EMBED_ONLY: AiProviderInfo = {
  ...OLLAMA,
  id: 'embed-host',
  name: 'Embed Host',
  models: [model('embed-host', 'nomic-embed-text', 'NONE')],
};

type Overrides = Record<string, TaskProviderPreference>;

/** Keeps the overrides in state the way `SettingsPage` does, and reports every change. */
const Harness: React.FC<{ initial?: Overrides; providers?: AiProviderInfo[]; onChange?: (o: Overrides) => void }> = ({
  initial = {},
  providers = [OLLAMA, GEMINI],
  onChange = (_next: Overrides) => {},
}) => {
  const [overrides, setOverrides] = useState<Overrides>(initial);
  return (
    <TaskRoutingChains
      providers={providers}
      overrides={overrides}
      onChange={(next) => {
        onChange(next);
        setOverrides(next);
      }}
    />
  );
};

const expand = () => fireEvent.click(document.getElementById('btn-toggle-task-routing')!);

const addEntry = (task: string, provider: string, modelId: string) => {
  fireEvent.change(document.getElementById(`task-chain-${task}-provider`)!, { target: { value: provider } });
  fireEvent.change(document.getElementById(`task-chain-${task}-model`)!, { target: { value: modelId } });
  fireEvent.click(document.getElementById(`task-chain-${task}-add`)!);
};

const GEN = 'TEST_CASE_GENERATION';

describe('TaskRoutingChains', () => {
  it('is collapsed when no task has a list, and expands to the four routed tasks, each on automatic', () => {
    render(<Harness />);
    expect(document.getElementById('task-chain-TEST_CASE_GENERATION')).toBeNull();

    expand();
    for (const label of ['Test case generation', 'Requirement analysis', 'Test case compilation', 'Bug analysis']) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    expect(screen.getAllByText('Automatic')).toHaveLength(4);
    expect(document.getElementById('btn-toggle-task-routing')!.getAttribute('aria-expanded')).toBe('true');
  });

  it('is already open when a task has a list, showing it in order with the first as primary', () => {
    render(
      <Harness
        initial={{
          [GEN]: {
            provider: null,
            model: null,
            chain: [
              { provider: 'ollama', model: 'qwen3:14b' },
              { provider: 'gemini', model: 'gemini-2.5-flash' },
            ],
          },
        }}
      />,
    );

    const card = document.getElementById(`task-chain-${GEN}`)!;
    const rows = within(card).getAllByRole('listitem');
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain('Ollama / qwen3:14b');
    expect(rows[0].textContent).toContain('primary');
    expect(rows[1].textContent).toContain('Gemini / gemini-2.5-flash');
    expect(rows[1].textContent).toContain('fallback');
    expect(within(card).queryByText('Automatic')).toBeNull();
    // A task that has no list stays automatic.
    expect(within(document.getElementById('task-chain-BUG_ANALYSIS')!).getByText('Automatic')).toBeTruthy();
  });

  it('adds a chosen provider/model to the end of the list and reports it under that task', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    expand();

    addEntry(GEN, 'ollama', 'qwen3:14b');
    addEntry(GEN, 'gemini', 'gemini-2.5-flash');

    expect(onChange).toHaveBeenLastCalledWith({
      [GEN]: {
        provider: null,
        model: null,
        chain: [
          { provider: 'ollama', model: 'qwen3:14b' },
          { provider: 'gemini', model: 'gemini-2.5-flash' },
        ],
      },
    });
    const rows = within(document.getElementById(`task-chain-${GEN}`)!).getAllByRole('listitem');
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining('Ollama / qwen3:14b'),
      expect.stringContaining('Gemini / gemini-2.5-flash'),
    ]);
  });

  it('cannot add until both a provider and a model are chosen, and never the same pair twice', () => {
    render(<Harness />);
    expand();
    const add = document.getElementById(`task-chain-${GEN}-add`) as HTMLButtonElement;
    const providerSelect = document.getElementById(`task-chain-${GEN}-provider`) as HTMLSelectElement;
    const modelSelect = document.getElementById(`task-chain-${GEN}-model`) as HTMLSelectElement;

    expect(add.disabled).toBe(true);
    expect(modelSelect.disabled).toBe(true);
    fireEvent.change(providerSelect, { target: { value: 'ollama' } });
    expect(modelSelect.disabled).toBe(false);
    expect(add.disabled).toBe(true);

    addEntry(GEN, 'ollama', 'qwen3:14b');
    fireEvent.change(modelSelect, { target: { value: 'qwen3:14b' } });
    expect(add.disabled).toBe(true);
    fireEvent.change(modelSelect, { target: { value: 'gemma3:12b' } });
    expect(add.disabled).toBe(false);
  });

  it('offers only models that can generate text, and hides a provider that has none', () => {
    render(<Harness providers={[OLLAMA, EMBED_ONLY]} />);
    expand();

    const providerSelect = document.getElementById(`task-chain-${GEN}-provider`) as HTMLSelectElement;
    expect(Array.from(providerSelect.options).map((o) => o.value)).toEqual(['', 'ollama']);

    fireEvent.change(providerSelect, { target: { value: 'ollama' } });
    const modelSelect = document.getElementById(`task-chain-${GEN}-model`) as HTMLSelectElement;
    expect(Array.from(modelSelect.options).map((o) => o.value)).toEqual(['', 'qwen3:14b', 'gemma3:12b']);
  });

  it('moves an entry up or down, and the first cannot go up nor the last down', () => {
    const onChange = vi.fn();
    render(
      <Harness
        onChange={onChange}
        initial={{
          [GEN]: {
            provider: null,
            model: null,
            chain: [
              { provider: 'ollama', model: 'qwen3:14b' },
              { provider: 'ollama', model: 'gemma3:12b' },
              { provider: 'gemini', model: 'gemini-2.5-flash' },
            ],
          },
        }}
      />,
    );

    expect((screen.getByLabelText('Move qwen3:14b up in Test case generation') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByLabelText('Move gemini-2.5-flash down in Test case generation') as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByLabelText('Move gemini-2.5-flash up in Test case generation'));
    expect(onChange.mock.lastCall![0][GEN].chain.map((t: { model: string }) => t.model)).toEqual([
      'qwen3:14b',
      'gemini-2.5-flash',
      'gemma3:12b',
    ]);

    fireEvent.click(screen.getByLabelText('Move qwen3:14b down in Test case generation'));
    expect(onChange.mock.lastCall![0][GEN].chain.map((t: { model: string }) => t.model)).toEqual([
      'gemini-2.5-flash',
      'qwen3:14b',
      'gemma3:12b',
    ]);
  });

  it('removing the last entry takes the task back to automatic and drops its override entirely', () => {
    const onChange = vi.fn();
    render(
      <Harness
        onChange={onChange}
        initial={{ [GEN]: { provider: null, model: null, chain: [{ provider: 'ollama', model: 'qwen3:14b' }] } }}
      />,
    );

    fireEvent.click(screen.getByLabelText('Remove qwen3:14b from Test case generation'));

    expect(onChange).toHaveBeenLastCalledWith({});
    expect(within(document.getElementById(`task-chain-${GEN}`)!).getByText('Automatic')).toBeTruthy();
  });

  it('leaves a pinned provider/model on the same task untouched when its list is edited or emptied', () => {
    const onChange = vi.fn();
    render(
      <Harness
        onChange={onChange}
        initial={{
          [GEN]: { provider: 'gemini', model: 'gemini-2.5-flash', chain: [{ provider: 'ollama', model: 'qwen3:14b' }] },
          BUG_ANALYSIS: { provider: 'ollama', model: null },
        }}
      />,
    );

    fireEvent.click(screen.getByLabelText('Remove qwen3:14b from Test case generation'));

    // Only the chain went; the pin on the same task and the other task's override are exactly as they were.
    expect(onChange).toHaveBeenLastCalledWith({
      [GEN]: { provider: 'gemini', model: 'gemini-2.5-flash' },
      BUG_ANALYSIS: { provider: 'ollama', model: null },
    });
  });

  it('keeps an entry whose model is not available right now, says it is skipped, and lets it be removed', () => {
    const onChange = vi.fn();
    render(
      <Harness
        onChange={onChange}
        initial={{
          [GEN]: {
            provider: null,
            model: null,
            chain: [
              { provider: 'home-pc', model: 'llama3:70b' },
              { provider: 'ollama', model: 'qwen3:14b' },
            ],
          },
        }}
      />,
    );

    const rows = within(document.getElementById(`task-chain-${GEN}`)!).getAllByRole('listitem');
    expect(rows[0].textContent).toContain('home-pc / llama3:70b');
    expect(rows[0].textContent).toContain('not available right now');
    expect(rows[1].textContent).not.toContain('not available right now');

    fireEvent.click(screen.getByLabelText('Remove llama3:70b from Test case generation'));
    expect(onChange.mock.lastCall![0][GEN].chain).toEqual([{ provider: 'ollama', model: 'qwen3:14b' }]);
  });

  it('marks a paid provider in the list but not a free one', () => {
    render(
      <Harness
        initial={{
          [GEN]: {
            provider: null,
            model: null,
            chain: [
              { provider: 'ollama', model: 'qwen3:14b' },
              { provider: 'gemini', model: 'gemini-2.5-flash' },
            ],
          },
        }}
      />,
    );

    const rows = within(document.getElementById(`task-chain-${GEN}`)!).getAllByRole('listitem');
    expect(within(rows[0]).queryByText('paid')).toBeNull();
    expect(within(rows[1]).getByText('paid').getAttribute('title')).toContain('$0.0003/1K tokens in');
  });
});
