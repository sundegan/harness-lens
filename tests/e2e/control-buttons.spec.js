/// <reference types="mocha" />
/// <reference types="webdriverio" />

import { $, browser, expect } from '@wdio/globals';

const settingsButton = '[data-testid="main-nav-settings"]';
const pinButton = '[data-testid="titlebar-pin-button"]';
const sessionsButton = '[data-testid="main-nav-sessions"]';
const skillsButton = '[data-testid="main-nav-skills"]';
const currentSessionsFilter = '[data-testid="session-filter-current"]';

async function expectSettingsPage() {
  await browser.waitUntil(async () => await $('[data-testid="settings-dialog"]').isDisplayed(), {
    timeoutMsg: 'settings dialog was not shown',
  });
  await expect($('[data-testid="settings-dialog"] [data-slot="field-label"]')).toHaveText(
    expect.stringMatching(/App Language|应用语言/)
  );
  await expect($('body')).not.toHaveText(expect.stringContaining('500 Internal Error'));
}

async function openRootPage() {
  const currentUrl = await browser.getUrl();
  await browser.url(new URL('/', currentUrl).toString());
}

describe('titlebar controls', () => {
  beforeEach(async () => {
    await openRootPage();
  });

  it('opens settings without a server error', async () => {
    await $(settingsButton).click();

    await expectSettingsPage();
    await $('[data-testid="settings-close-button"]').click();
    await browser.waitUntil(
      async () => !(await $('[data-testid="settings-dialog"]').isDisplayed()),
      { timeoutMsg: 'settings dialog was not closed' }
    );
    await expect($(sessionsButton)).toHaveAttribute('aria-current', 'page');
  });

  it('toggles the real Tauri always-on-top window state', async () => {
    const button = await $(pinButton);
    const previousLabel = await button.getAttribute('aria-label');

    await button.click();

    await browser.waitUntil(
      async () => (await button.getAttribute('aria-label')) !== previousLabel,
      {
        timeoutMsg: 'pin button did not reflect the updated Tauri window state',
      }
    );
  });
});

describe('main workspace', () => {
  beforeEach(async () => {
    await openRootPage();
  });

  it('switches between session and skill analysis modules', async () => {
    await browser.waitUntil(
      () =>
        browser.execute(
          (selector) => document.querySelector(selector)?.getAttribute('aria-current') === 'page',
          sessionsButton
        ),
      { timeoutMsg: 'session module was not active' }
    );

    await browser.execute((selector) => {
      const element = document.querySelector(selector);
      if (element instanceof HTMLElement) element.click();
    }, skillsButton);
    await browser.waitUntil(
      () =>
        browser.execute(
          (selector) => document.querySelector(selector)?.getAttribute('aria-current') === 'page',
          skillsButton
        ),
      { timeoutMsg: 'skill module was not active' }
    );

    await browser.execute((selector) => {
      const element = document.querySelector(selector);
      if (element instanceof HTMLElement) element.click();
    }, sessionsButton);
    await browser.waitUntil(
      () =>
        browser.execute(
          (selector) => document.querySelector(selector)?.getAttribute('aria-current') === 'page',
          sessionsButton
        ),
      { timeoutMsg: 'session module did not become active again' }
    );

    await browser.execute((selector) => {
      const element = document.querySelector(selector);
      if (element instanceof HTMLElement) element.click();
    }, currentSessionsFilter);
    await browser.waitUntil(
      () =>
        browser.execute(
          (selector) => document.querySelector(selector)?.getAttribute('data-state') === 'on',
          currentSessionsFilter
        ),
      { timeoutMsg: 'current-session filter was not selected' }
    );
    const bodyText = await browser.execute(() => document.body.textContent ?? '');
    expect(bodyText).not.toContain('500 Internal Error');
  });
});
