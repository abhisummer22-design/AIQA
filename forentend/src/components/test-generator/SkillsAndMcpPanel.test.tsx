/**
 * Live "which skill is running right now" indicator (`activeSkillNames`
 * prop) — a focused, standalone component test. `SkillsAndMcpPanel` takes
 * only plain props/callbacks, so it's rendered directly here, no providers
 * or fetch mocking needed (`ApplicationSourceSection.test.tsx` established
 * this same "render the component directly" convention for a props-only
 * component in this same folder).
 */
import React from 'react';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SkillsAndMcpPanel } from './SkillsAndMcpPanel';
import type { SkillDefinition } from '../../types/skills';

function skill(overrides: Partial<SkillDefinition> = {}): SkillDefinition {
  return {
    id: 'skill-functional-testing',
    name: 'Functional Testing',
    description: 'Covers real functional flows.',
    type: 'BUILT_IN',
    availability: 'AVAILABLE',
    configurable: true,
    ...overrides,
  };
}

const SKILLS: SkillDefinition[] = [
  skill({ id: 'skill-functional-testing', name: 'Functional Testing' }),
  skill({ id: 'skill-security-testing', name: 'Security Testing' }),
  skill({ id: 'skill-rbac-testing', name: 'RBAC Testing' }),
];

function noop() {}
async function noopAdd(): Promise<SkillDefinition> {
  return skill();
}

describe('SkillsAndMcpPanel — live wave/skill indicator', () => {
  it('glows only the real skill(s) named in activeSkillNames, none of the others', () => {
    render(
      <SkillsAndMcpPanel
        skills={SKILLS}
        selectedSkillIds={['skill-functional-testing', 'skill-security-testing', 'skill-rbac-testing']}
        onToggleSkill={noop}
        onSelectAll={noop}
        onDeselectAll={noop}
        onAddCustomSkill={noopAdd}
        activeSkillNames={['Functional Testing']}
      />
    );

    expect(document.querySelector('#skill-row-skill-functional-testing')?.getAttribute('data-active-now')).toBe(
      'true'
    );
    expect(document.querySelector('#skill-row-skill-security-testing')?.getAttribute('data-active-now')).toBe(
      'false'
    );
    expect(document.querySelector('#skill-row-skill-rbac-testing')?.getAttribute('data-active-now')).toBe('false');

    expect(document.getElementById('skill-active-badge-skill-functional-testing')).toBeTruthy();
    expect(document.getElementById('skill-active-badge-skill-security-testing')).toBeNull();
  });

  it('glows every real skill in a multi-skill wave at once, since a wave runs them all in one combined call', () => {
    render(
      <SkillsAndMcpPanel
        skills={SKILLS}
        selectedSkillIds={['skill-security-testing', 'skill-rbac-testing']}
        onToggleSkill={noop}
        onSelectAll={noop}
        onDeselectAll={noop}
        onAddCustomSkill={noopAdd}
        activeSkillNames={['Security Testing', 'RBAC Testing']}
      />
    );

    expect(document.querySelector('#skill-row-skill-security-testing')?.getAttribute('data-active-now')).toBe(
      'true'
    );
    expect(document.querySelector('#skill-row-skill-rbac-testing')?.getAttribute('data-active-now')).toBe('true');
    expect(document.querySelector('#skill-row-skill-functional-testing')?.getAttribute('data-active-now')).toBe(
      'false'
    );
  });

  it('glows nothing when no generation is in flight (the default, empty activeSkillNames)', () => {
    render(
      <SkillsAndMcpPanel
        skills={SKILLS}
        selectedSkillIds={['skill-functional-testing']}
        onToggleSkill={noop}
        onSelectAll={noop}
        onDeselectAll={noop}
        onAddCustomSkill={noopAdd}
      />
    );

    for (const s of SKILLS) {
      expect(document.querySelector(`#skill-row-${s.id}`)?.getAttribute('data-active-now')).toBe('false');
    }
    expect(document.querySelectorAll('[id^="skill-active-badge-"]').length).toBe(0);
  });
});

describe('SkillsAndMcpPanel — Select All / Deselect All', () => {
  it('shows the real eligible count and calls the real handler when nothing is selected yet', () => {
    const onSelectAll = vi.fn();
    render(
      <SkillsAndMcpPanel
        skills={SKILLS}
        selectedSkillIds={[]}
        onToggleSkill={noop}
        onSelectAll={onSelectAll}
        onDeselectAll={noop}
        onAddCustomSkill={noopAdd}
      />
    );

    expect(screen.getByText(`Select All (${SKILLS.length})`)).toBeTruthy();
    expect(document.getElementById('btn-deselect-all-skills')).toBeNull();

    document.getElementById('btn-select-all-skills')!.click();
    expect(onSelectAll).toHaveBeenCalledTimes(1);
  });

  it('switches to Deselect All once every real eligible skill is already selected, and calls the real handler', () => {
    const onDeselectAll = vi.fn();
    render(
      <SkillsAndMcpPanel
        skills={SKILLS}
        selectedSkillIds={SKILLS.map((s) => s.id)}
        onToggleSkill={noop}
        onSelectAll={noop}
        onDeselectAll={onDeselectAll}
        onAddCustomSkill={noopAdd}
      />
    );

    expect(screen.getByText('Deselect All')).toBeTruthy();
    expect(document.getElementById('btn-select-all-skills')).toBeNull();

    document.getElementById('btn-deselect-all-skills')!.click();
    expect(onDeselectAll).toHaveBeenCalledTimes(1);
  });

  it('a NOT_CONFIGURED skill never counts as eligible, so selecting every OTHER real skill still shows Deselect All', () => {
    const skillsWithOneUnavailable: SkillDefinition[] = [
      ...SKILLS,
      skill({ id: 'skill-unavailable', name: 'Unavailable Skill', availability: 'NOT_CONFIGURED' }),
    ];
    render(
      <SkillsAndMcpPanel
        skills={skillsWithOneUnavailable}
        selectedSkillIds={SKILLS.map((s) => s.id)}
        onToggleSkill={noop}
        onSelectAll={noop}
        onDeselectAll={noop}
        onAddCustomSkill={noopAdd}
      />
    );

    // Every real, selectable skill IS selected (the unavailable one can
    // never be, by design) — this must already read as "all selected",
    // not stuck showing "Select All" because of a skill nobody could
    // ever actually select.
    expect(screen.getByText('Deselect All')).toBeTruthy();
  });
});
