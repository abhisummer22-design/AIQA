import { describe, expect, it } from 'vitest';
import { BUILT_IN_SKILLS, categoryMatchedSkillIds } from './skillRegistry';
import type { SkillDefinition } from '../types/skills';

function skill(id: string, name: string, type: SkillDefinition['type'] = 'BUILT_IN'): SkillDefinition {
  return { id, name, description: '', type, availability: 'AVAILABLE', configurable: true };
}

describe('categoryMatchedSkillIds', () => {
  it('matches a skill to a category naming the same concept, stripping the generic testing-verb suffix', () => {
    const skills = [skill('skill-performance-testing', 'Performance Testing')];
    const matched = categoryMatchedSkillIds(['Performance'], skills);
    expect(matched.get('skill-performance-testing')).toBe('Performance');
  });

  it('matches "Analysis" the same way as "Testing"', () => {
    const skills = [skill('skill-edge-case-analysis', 'Edge Case Analysis')];
    const matched = categoryMatchedSkillIds(['Edge Case'], skills);
    expect(matched.has('skill-edge-case-analysis')).toBe(true);
  });

  it('never matches a skill with no real category counterpart', () => {
    const skills = [skill('skill-database-testing', 'Database Testing'), skill('skill-rbac-testing', 'RBAC Testing')];
    const matched = categoryMatchedSkillIds(['Functional', 'Security', 'API'], skills);
    expect(matched.size).toBe(0);
  });

  it('matches "Business Logic" to "Business Case Testing" via the curated alias, not a general fuzzy guess', () => {
    const skills = [skill('skill-business-case-testing', 'Business Case Testing')];
    const matched = categoryMatchedSkillIds(['Business Logic'], skills);
    expect(matched.get('skill-business-case-testing')).toBe('Business Logic');
  });

  it('still never fuzzy-matches an unrelated near-miss name with no real alias', () => {
    const skills = [skill('skill-database-testing', 'Database Testing')];
    // "Data" is a real near-miss of "Database" but the two are not the same
    // concept, and no alias exists for it — must stay unmatched, proving
    // the alias table is a small, curated exception list, not a general
    // fuzzy-matching mechanism.
    const matched = categoryMatchedSkillIds(['Data'], skills);
    expect(matched.size).toBe(0);
  });

  it('only matches categories that are actually selected', () => {
    const skills = [skill('skill-security-testing', 'Security Testing')];
    expect(categoryMatchedSkillIds([], skills).size).toBe(0);
    expect(categoryMatchedSkillIds(['Performance'], skills).size).toBe(0);
  });

  it('matches a real file-based skill exactly like a built-in one, by name alone', () => {
    const skills = [skill('file-skill-performance-md', 'Performance Testing', 'FILE')];
    const matched = categoryMatchedSkillIds(['Performance'], skills);
    expect(matched.get('file-skill-performance-md')).toBe('Performance');
  });

  it('is case-insensitive on both the category and the skill name', () => {
    const skills = [skill('skill-api-testing', 'api testing')];
    const matched = categoryMatchedSkillIds(['API'], skills);
    expect(matched.has('skill-api-testing')).toBe(true);
  });

  /** Phase 1 gap-closing — these 5 categories previously had no real skill
   *  counterpart at all (this file's own doc comment named the gap
   *  explicitly). Uses the REAL `BUILT_IN_SKILLS` array, not a hand-built
   *  fake skill list, so this proves the actual shipped registry — not
   *  just the matching function in isolation. */
  it('matches each of the 5 newly-added built-in skills to its own real category, using the real registry', () => {
    const cases: Array<[string, string]> = [
      ['UI/UX', 'skill-ui-ux-testing'],
      ['Negative', 'skill-negative-testing'],
      ['Positive', 'skill-positive-testing'],
      ['Spike', 'skill-spike-testing'],
      ['Load', 'skill-load-testing'],
    ];
    for (const [category, skillId] of cases) {
      const matched = categoryMatchedSkillIds([category], BUILT_IN_SKILLS);
      expect(matched.get(skillId), `category "${category}" should match ${skillId}`).toBe(category);
    }
  });

  /** Phase 4 — the "Non-Functional" category previously had no skill, so
   *  ticking it (it is ticked by default) generated nothing of the kind. */
  it('matches the Non-Functional category to its own real skill, using the real registry', () => {
    const matched = categoryMatchedSkillIds(['Non-Functional'], BUILT_IN_SKILLS);
    expect(matched.get('skill-non-functional-testing')).toBe('Non-Functional');
    expect(matched.size).toBe(1);
    expect(BUILT_IN_SKILLS.find((s) => s.id === 'skill-non-functional-testing')?.name).toBe('Non-Functional Testing');
    // Not there when the category is not selected.
    expect(categoryMatchedSkillIds(['Functional'], BUILT_IN_SKILLS).has('skill-non-functional-testing')).toBe(false);
  });

  it('keeps every built-in skill id unique', () => {
    const ids = BUILT_IN_SKILLS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});
