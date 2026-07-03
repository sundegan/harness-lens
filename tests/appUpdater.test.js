import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('appUpdateManager state machine contains expected properties and mock mode support', () => {
  const updateSvelte = readFileSync(
    new URL('../src/lib/update.svelte.ts', import.meta.url),
    'utf8'
  );

  // States
  assert.match(updateSvelte, /currentVersion\s*=\s*\$state\(/);
  assert.match(updateSvelte, /latestVersion\s*=\s*\$state\(/);
  assert.match(updateSvelte, /status\s*=\s*\$state</);
  assert.match(updateSvelte, /error\s*=\s*\$state\(/);
  assert.match(updateSvelte, /autoCheckUpdates\s*=\s*\$state\(/);
  assert.match(updateSvelte, /const\s+DEFAULT_AUTO_CHECK_UPDATES\s*=\s*true/);
  assert.match(updateSvelte, /String\(DEFAULT_AUTO_CHECK_UPDATES\)\)\s*!==\s*'false'/);

  // Derived properties
  assert.match(updateSvelte, /hasUpdate\s*=\s*\$derived\(/);
  assert.match(updateSvelte, /isChecking\s*=\s*\$derived\(/);

  // Methods
  assert.match(updateSvelte, /async\s+init\(\)/);
  assert.match(updateSvelte, /setAutoCheckUpdates\(/);
  assert.match(updateSvelte, /async\s+checkForUpdates\(/);
  assert.match(updateSvelte, /async\s+installUpdate\(\)/);

  // Mock Mode
  assert.match(updateSvelte, /function\s+isMockAppUpdateEnabled\(\)/);
  assert.match(updateSvelte, /VITE_MOCK_APP_UPDATE/);
  assert.match(updateSvelte, /mockUpdate|mock-app-update/);
  assert.match(updateSvelte, /9\.9\.9-dev/);
});
