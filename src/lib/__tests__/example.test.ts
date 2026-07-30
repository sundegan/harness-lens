import { describe, expect, it } from 'vitest';

describe('示例测试', () => {
  it('应该通过基本断言', () => {
    expect(1 + 1).toBe(2);
  });

  it('应该测试字符串', () => {
    const str = 'HarnessLens';
    expect(str).toContain('Lens');
  });
});
