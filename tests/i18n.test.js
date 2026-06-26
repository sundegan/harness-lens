import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('i18nManager contains expected reactive states, dictionaries and translation engine', () => {
  const i18nSvelte = readFileSync(
    new URL('../src/lib/i18n.svelte.ts', import.meta.url),
    'utf8'
  );

  // Dictionaries
  assert.match(i18nSvelte, /dictionaries\s*=\s*\{/);
  assert.match(i18nSvelte, /'main\.title':\s*'/);
  assert.match(i18nSvelte, /'settings\.language\.title':\s*'/);

  // States & Getters/Setters
  assert.match(i18nSvelte, /#language\s*=\s*\$state</);
  assert.match(i18nSvelte, /get\s+language\(\)/);
  assert.match(i18nSvelte, /set\s+language\(/);

  // Derived resolved language
  assert.match(i18nSvelte, /resolvedLanguage\s*=\s*\$derived\.by/);

  // Translation method
  assert.match(i18nSvelte, /t\(key:\s*string,\s*data\?/);
  assert.match(i18nSvelte, /text\.replace/);

  // System language detection
  assert.match(i18nSvelte, /function\s+getSystemLanguage\(\)/);
  assert.match(i18nSvelte, /navigator\.language/);
});
