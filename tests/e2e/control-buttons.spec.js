import { expect } from '@wdio/globals';

const settingsButton = '[data-testid="titlebar-settings-button"]';
const updateButton = '[data-testid="titlebar-update-button"]';
const pinButton = '[data-testid="titlebar-pin-button"]';

async function expectSettingsPage() {
  await browser.waitUntil(async () => (await browser.getUrl()).endsWith('/settings'), {
    timeoutMsg: 'settings route was not opened'
  });

  await browser.waitUntil(
    async () => ['General Settings', '通用设置'].includes(await $('h3').getText()),
    {
      timeoutMsg: 'settings heading was not shown'
    }
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
  });

  it('opens settings from the update button when an update is available', async () => {
    await $(updateButton).waitForDisplayed();
    await $(updateButton).click();

    await expectSettingsPage();
  });

  it('toggles the real Tauri always-on-top window state', async () => {
    const button = await $(pinButton);
    const wasActive = (await button.getAttribute('class')).includes('is-active');

    await button.click();

    await browser.waitUntil(
      async () => {
        const className = await button.getAttribute('class');
        return className.includes('is-active') !== wasActive;
      },
      {
        timeoutMsg: 'pin button did not reflect the updated Tauri window state'
      }
    );
  });
});
