import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

test('native menu check update event and frontend layout wiring', () => {
  const menuRs = readFileSync(new URL('../src-tauri/src/menu.rs', import.meta.url), 'utf8');
  const libRs = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  const layoutSvelte = readFileSync(new URL('../src/routes/+layout.svelte', import.meta.url), 'utf8');

  // Rust Menu
  assert.match(menuRs, /SETTINGS_MENU_ID/);
  assert.match(menuRs, /OPEN_SETTINGS_EVENT/);
  assert.match(menuRs, /"Settings\.\.\."/);
  assert.match(menuRs, /CHECK_UPDATES_MENU_ID/);
  assert.match(menuRs, /CHECK_UPDATES_EVENT/);
  assert.match(menuRs, /"Check for Updates\.\.\."/);

  // Rust event binding
  assert.match(menuRs, /pub\s+fn\s+handle_menu_event/);
  assert.match(menuRs, /SETTINGS_MENU_ID\s*=>\s*OPEN_SETTINGS_EVENT/);
  assert.match(menuRs, /CHECK_UPDATES_MENU_ID\s*=>\s*CHECK_UPDATES_EVENT/);
  assert.match(libRs, /\.on_menu_event\(menu::handle_menu_event\)/);

  // Frontend layout listener
  assert.match(layoutSvelte, /import \{ listen \} from '@tauri-apps\/api\/event'/);
  assert.match(layoutSvelte, /listen\('open-settings'/);
  assert.match(layoutSvelte, /listen\('check-for-updates'/);
  assert.match(layoutSvelte, /appUpdateManager\.checkForUpdates\(\)/);
  assert.match(layoutSvelte, /goto\('\/settings'\)/);
  assert.match(layoutSvelte, /unlistenSettings/);
  assert.match(layoutSvelte, /unlistenUpdate/);
});
