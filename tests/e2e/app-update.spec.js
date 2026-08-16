/// <reference types="mocha" />
/// <reference types="webdriverio" />

import { $, browser, expect } from '@wdio/globals';

async function openMockUpdate(mode = '1') {
  const currentUrl = await browser.getUrl();
  await browser.url(new URL(`/?mockUpdate=${mode}`, currentUrl).toString());
  await browser.refresh();
  await browser.waitUntil(
    () =>
      browser.execute(
        (expectedMode) =>
          document.querySelector(
            expectedMode === 'latest'
              ? '[data-testid="main-nav-settings"]'
              : '[data-testid="main-nav-update"]'
          ) instanceof HTMLElement,
        mode
      ),
    { timeoutMsg: 'mock update entry was not shown' }
  );
}

describe('application updates', () => {
  it('reports that the app is up to date after a manual check', async () => {
    await openMockUpdate('latest');

    await $('[data-testid="main-nav-settings"]').click();
    await $('[data-testid="settings-nav-updates"]').click();
    await expect($('#auto-check-updates')).toExist();
    await expect($('#current-app-version')).toHaveText(expect.stringMatching(/\d+\.\d+\.\d+/));

    const autoCheckUpdates = $('#auto-check-updates');
    const wasAutoCheckEnabled = (await autoCheckUpdates.getAttribute('data-state')) === 'checked';
    if (wasAutoCheckEnabled) {
      await autoCheckUpdates.click();
    }
    await expect($('#auto-check-updates')).toHaveAttribute('data-state', 'unchecked');
    await browser.waitUntil(
      () =>
        browser.execute(
          () => document.querySelector('[data-testid="check-for-updates"]') instanceof HTMLElement
        ),
      { timeoutMsg: 'manual update check button was not shown' }
    );
    await $('[data-testid="check-for-updates"]').click();

    await browser.waitUntil(
      () =>
        browser
          .execute(() =>
            document.querySelector('[data-testid="update-status"]')?.textContent?.trim()
          )
          .then((status) => /Up to date|已是最新版本/.test(status ?? '')),
      { timeoutMsg: 'latest-version status was not shown after manual check' }
    );

    if (wasAutoCheckEnabled) {
      await autoCheckUpdates.click();
      await expect(autoCheckUpdates).toHaveAttribute('data-state', 'checked');
    }
  });

  it('downloads the mock update and allows deferring the restart', async () => {
    await openMockUpdate();

    await $('[data-testid="main-nav-update"]').click();
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="app-update-dialog"]')
              ?.getAttribute('data-state') === 'open'
        ),
      { timeoutMsg: 'mock update dialog was not shown' }
    );

    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow') !== null
        ),
      { timeoutMsg: 'mock update progress was not shown' }
    );
    await expect($('[role="progressbar"]')).toHaveAttribute('aria-valuenow');

    await browser.waitUntil(
      () =>
        browser.execute(() =>
          Boolean(document.querySelector('[data-testid="update-restart-now"]'))
        ),
      { timeoutMsg: 'mock update did not finish downloading' }
    );
    await expect($('[data-testid="app-update-dialog"]')).toHaveText(
      expect.stringMatching(/9\.9\.9-dev|9\.9\.9/)
    );

    await $('[data-testid="update-later"]').click();
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="app-update-dialog"]')
              ?.getAttribute('data-state') !== 'open'
        ),
      { timeoutMsg: 'mock update dialog was not closed' }
    );
  });

  it('restarts the mock app after confirming installation', async () => {
    await openMockUpdate();

    await $('[data-testid="main-nav-update"]').click();
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="app-update-dialog"]')
              ?.getAttribute('data-state') === 'open'
        ),
      { timeoutMsg: 'mock update dialog was not shown for restart flow' }
    );
    await browser.waitUntil(
      () =>
        browser.execute(() =>
          Boolean(document.querySelector('[data-testid="update-restart-now"]'))
        ),
      { timeoutMsg: 'mock update did not finish downloading for restart flow' }
    );

    await $('[data-testid="update-restart-now"]').click();
    await browser.waitUntil(
      () =>
        browser.execute(
          () => document.querySelector('[data-testid="main-nav-update"]') instanceof HTMLElement
        ),
      { timeout: 15000, timeoutMsg: 'mock app did not reload after restart confirmation' }
    );
  });
});
