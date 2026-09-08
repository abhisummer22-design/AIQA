import { describe, expect, it } from 'vitest';
import { categoryMatchedSkillIds } from './skillRegistry';
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
});
