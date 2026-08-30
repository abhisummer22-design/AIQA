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

  it('never fuzzy-matches a near-miss name — "Business Logic" category does not pull in "Business Case Testing"', () => {
    const skills = [skill('skill-business-case-testing', 'Business Case Testing')];
    const matched = categoryMatchedSkillIds(['Business Logic'], skills);
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
