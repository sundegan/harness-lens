import { describe, expect, it } from 'vitest';
import { normalizePageInput } from '$lib/pagination';

describe('normalizePageInput', () => {
  it('accepts valid integer pages', () => {
    expect(normalizePageInput(' 3 ', 10)).toBe(3);
  });

  it('clamps pages to the available range', () => {
    expect(normalizePageInput('0', 10)).toBe(1);
    expect(normalizePageInput('99', 10)).toBe(10);
  });

  it('rejects empty, fractional, and nonnumeric values', () => {
    expect(normalizePageInput('', 10)).toBeNull();
    expect(normalizePageInput('2.5', 10)).toBeNull();
    expect(normalizePageInput('next', 10)).toBeNull();
  });
});
