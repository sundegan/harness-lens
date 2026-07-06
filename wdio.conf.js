import path from 'node:path';
import { fileURLToPath } from 'node:url';

const rootDir = path.dirname(fileURLToPath(import.meta.url));
const appName = process.platform === 'win32' ? 'codex-timeline.exe' : 'codex-timeline';
const appBinaryPath = path.join(rootDir, 'src-tauri', 'target', 'debug', appName);

export const config = {
  runner: 'local',
  specs: ['./tests/e2e/**/*.spec.js'],
  maxInstances: 1,
  logLevel: 'warn',
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 1,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000
  },
  services: [
    [
      '@wdio/tauri-service',
      {
        appBinaryPath,
        driverProvider: 'embedded',
        embeddedPort: Number(process.env.TAURI_WEBDRIVER_PORT ?? 4445),
        startTimeout: 120000,
        captureFrontendLogs: true,
        captureBackendLogs: true
      }
    ]
  ],
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: appBinaryPath
      }
    }
  ]
};
