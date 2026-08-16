/// <reference types="mocha" />
/// <reference types="webdriverio" />

import { $, browser, expect } from '@wdio/globals';

const settingsButton = '[data-testid="main-nav-settings"]';
const pinButton = '[data-testid="titlebar-pin-button"]';
const sidebarToggleButton = '[data-testid="titlebar-sidebar-toggle"]';
const sessionsButton = '[data-testid="main-nav-sessions"]';
const skillsButton = '[data-testid="main-nav-skills"]';
const toolCallsButton = '[data-testid="main-nav-tool-calls"]';
const currentSessionsFilter = '[data-testid="session-filter-current"]';

async function expectSettingsPage() {
  await browser.waitUntil(
    async () => {
      const isOpen = await browser.execute(
        () =>
          document.querySelector('[data-testid="settings-dialog"]')?.getAttribute('data-state') ===
          'open'
      );
      if (isOpen) return true;
      await clickSelector(settingsButton);
      return false;
    },
    { timeoutMsg: 'settings dialog was not shown' }
  );
  await expect($('[data-testid="settings-dialog"] [data-slot="field-label"]')).toHaveText(
    expect.stringMatching(/App Language|应用语言/)
  );
  await expect($('body')).not.toHaveText(expect.stringContaining('500 Internal Error'));
}

async function openRootPage() {
  const currentUrl = await browser.getUrl();
  await browser.url(new URL('/', currentUrl).toString());
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector) => document.querySelector(selector)?.getAttribute('aria-current') === 'page',
        sessionsButton
      ),
    { timeoutMsg: 'root workspace was not ready' }
  );
}

/** @param {string} selector */
async function clickSelector(selector) {
  await browser.waitUntil(
    () =>
      browser.execute(
        /** @param {string} value */
        (value) => document.querySelector(value) instanceof HTMLElement,
        selector
      ),
    { timeoutMsg: `element was not ready: ${selector}` }
  );
  await browser.execute(
    /** @param {string} value */
    (value) => {
      const element = document.querySelector(value);
      if (element instanceof HTMLElement) element.click();
    },
    selector
  );
}

describe('titlebar controls', () => {
  beforeEach(async () => {
    await openRootPage();
  });

  it('opens settings without a server error', async () => {
    await clickSelector(settingsButton);

    await expectSettingsPage();
    await $('[data-testid="settings-close-button"]').click();
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="settings-dialog"]')
              ?.getAttribute('data-state') !== 'open'
        ),
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

  it('closes the sidebar tooltip after clicking without moving the pointer', async () => {
    const button = await $(sidebarToggleButton);
    await browser.execute((selector) => {
      const element = document.querySelector(selector);
      if (!(element instanceof HTMLElement)) return;
      element.dispatchEvent(new PointerEvent('pointerenter', { pointerType: 'mouse' }));
      element.dispatchEvent(
        new PointerEvent('pointermove', { bubbles: true, pointerType: 'mouse' })
      );
    }, sidebarToggleButton);

    await browser.waitUntil(
      () =>
        browser.execute((selector) => {
          const state = document.querySelector(selector)?.getAttribute('data-state');
          return state === 'delayed-open' || state === 'instant-open';
        }, sidebarToggleButton),
      { timeoutMsg: 'sidebar tooltip did not open after hovering the toggle button' }
    );

    await button.click();
    try {
      await browser.waitUntil(
        () =>
          browser.execute(
            (selector) => document.querySelector(selector)?.getAttribute('data-state') === 'closed',
            sidebarToggleButton
          ),
        { timeoutMsg: 'sidebar tooltip remained open after clicking the toggle button' }
      );
    } finally {
      // Restore the initial sidebar state for the following titlebar scenarios.
      await button.click();
      await browser.waitUntil(
        () =>
          browser.execute(
            (selector) =>
              document.querySelector(selector)?.getAttribute('aria-expanded') === 'true',
            sidebarToggleButton
          ),
        { timeoutMsg: 'sidebar did not return to its initial expanded state' }
      );
    }
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

  it('opens the complete tool-call analysis workspace', async () => {
    await clickSelector(toolCallsButton);
    await browser.waitUntil(
      () =>
        browser.execute(
          (selector) => document.querySelector(selector)?.getAttribute('aria-current') === 'page',
          toolCallsButton
        ),
      { timeoutMsg: 'tool-call analysis module was not active' }
    );
    await expect($('[data-testid="tool-call-analysis"]')).toBeDisplayed();
    await expect($('[data-testid="tool-call-trend"]')).toBeDisplayed();
    await clickSelector('[data-testid="tool-call-view-comparisons"]');
    await expect($('[data-testid="tool-call-comparison-provider"]')).toHaveText(
      expect.stringContaining('codex')
    );
    await expect($('[data-testid="tool-call-comparison-provider"]')).toHaveText(
      expect.stringContaining('claude-code')
    );
    await expect($('[data-testid="tool-call-comparison-version"]')).not.toExist();
    await expect($('[data-testid="tool-call-comparison-project"]')).toHaveText(
      expect.stringContaining('Fixture Alpha')
    );
    await expect($('[data-testid="tool-call-comparison-mcp"]')).toHaveText(
      expect.stringContaining('filesystem')
    );

    await clickSelector('[data-testid="tool-call-view-calls"]');
    await expect($('[data-testid="tool-call-row-e2e-call-3"]')).toBeDisplayed();
    await expect($('[data-testid="tool-call-status-e2e-call-3"]')).toHaveText('completed');
    const columnOffsets = await browser.execute(() => {
      /** @param {string} selector */
      const left = (selector) =>
        document.querySelector(selector)?.getBoundingClientRect().left ?? 0;
      return {
        tool:
          left('[data-testid="tool-call-tool-e2e-call-3"]') -
          left('[data-testid="tool-call-header-tool"]'),
        provider:
          left('[data-testid="tool-call-provider-e2e-call-3"]') -
          left('[data-testid="tool-call-header-provider"]'),
      };
    });
    expect(Math.abs(columnOffsets.tool)).toBeLessThan(1);
    expect(Math.abs(columnOffsets.provider)).toBeLessThan(1);
    await browser.execute(() => {
      const target = document.querySelector('[data-testid="tool-call-session-title-e2e-call-3"]');
      target?.dispatchEvent(
        new PointerEvent('pointerenter', { bubbles: false, pointerType: 'mouse' })
      );
    });
    await expect($('[data-testid="tool-call-session-title-tooltip-e2e-call-3"]')).toHaveText(
      'E2E Codex session'
    );
    await clickSelector('[data-testid="tool-call-row-e2e-call-3"]');
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="tool-call-detail"]')
              ?.getAttribute('data-state') === 'open'
        ),
      { timeoutMsg: 'tool-call detail did not open' }
    );
    await expect($('[data-testid="tool-call-detail-session-title"]')).toHaveText(
      'E2E Codex session'
    );
    await expect($('[data-testid="tool-call-detail-session-id"]')).toHaveText(
      'Session ID: codex-e2e'
    );
    const sessionIdSelection = await browser.execute(() => {
      const element = document.querySelector('[data-testid="tool-call-detail-session-id"]');
      if (!(element instanceof HTMLElement)) return null;
      const style = getComputedStyle(element);
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(element);
      selection?.removeAllRanges();
      selection?.addRange(range);
      const selectedText = selection?.toString() ?? '';
      const clipboardData = new DataTransfer();
      window.dispatchEvent(new ClipboardEvent('copy', { clipboardData }));
      const copiedText = clipboardData.getData('text/plain');
      selection?.removeAllRanges();
      return {
        copiedText,
        userSelect: style.userSelect || style.getPropertyValue('-webkit-user-select'),
        selectedText,
      };
    });
    expect(sessionIdSelection).not.toBeNull();
    expect(sessionIdSelection?.copiedText).toBe('Session ID: codex-e2e');
    expect(sessionIdSelection?.userSelect).toBe('text');
    expect(sessionIdSelection?.selectedText).toBe('Session ID: codex-e2e');
    await expect($('[data-testid="tool-call-detail"]')).not.toHaveText(
      expect.stringContaining('e2e-session-codex')
    );
    await expect($('[data-testid="tool-call-detail-resize"]')).toHaveAttribute(
      'aria-valuemin',
      '672'
    );
    await expect($('[data-testid="tool-call-detail-resize"]')).toHaveAttribute(
      'aria-valuemax',
      '1152'
    );
    await expect($('[data-testid="tool-call-detail-resize"]')).toHaveAttribute(
      'aria-valuenow',
      '672'
    );
    const detailWidthBeforeDrag = await browser.execute(
      () =>
        document.querySelector('[data-testid="tool-call-detail"]')?.getBoundingClientRect().width ??
        0
    );
    await browser.execute(() => {
      const handle = document.querySelector('[data-testid="tool-call-detail-resize"]');
      const rect = handle?.getBoundingClientRect();
      if (!handle || !rect) return;
      const startX = rect.left + rect.width / 2;
      const startY = rect.top + rect.height / 2;
      handle.setAttribute('data-e2e-start-x', String(startX));
      handle.setAttribute('data-e2e-start-y', String(startY));
      handle.dispatchEvent(
        new MouseEvent('mousedown', {
          bubbles: true,
          button: 0,
          buttons: 1,
          clientX: startX,
          clientY: startY,
        })
      );
    });
    await expect($('[data-testid="tool-call-detail-resize-overlay"]')).toExist();
    await browser.execute(() => {
      const handle = document.querySelector('[data-testid="tool-call-detail-resize"]');
      const startX = Number(handle?.getAttribute('data-e2e-start-x') ?? 0);
      const startY = Number(handle?.getAttribute('data-e2e-start-y') ?? 0);
      window.dispatchEvent(
        new MouseEvent('mousemove', {
          bubbles: true,
          buttons: 1,
          clientX: startX - 80,
          clientY: startY,
        })
      );
    });
    const resizeMetricsAfterMove = await browser.execute(() => ({
      ariaValue: document
        .querySelector('[data-testid="tool-call-detail-resize"]')
        ?.getAttribute('aria-valuenow'),
      inlineWidth: document
        .querySelector('[data-testid="tool-call-detail"]')
        ?.getAttribute('style'),
      layoutWidth:
        document.querySelector('[data-testid="tool-call-detail"]')?.getBoundingClientRect().width ??
        0,
    }));
    await browser.waitUntil(
      () =>
        browser.execute(
          (width) =>
            (document.querySelector('[data-testid="tool-call-detail"]')?.getBoundingClientRect()
              .width ?? 0) >
            width + 40,
          detailWidthBeforeDrag
        ),
      {
        timeoutMsg: `tool-call detail panel did not respond to mouse dragging: ${JSON.stringify(resizeMetricsAfterMove)}`,
      }
    );
    await browser.execute(() => {
      const handle = document.querySelector('[data-testid="tool-call-detail-resize"]');
      const startX = Number(handle?.getAttribute('data-e2e-start-x') ?? 0);
      const startY = Number(handle?.getAttribute('data-e2e-start-y') ?? 0);
      window.dispatchEvent(
        new MouseEvent('mouseup', {
          bubbles: true,
          button: 0,
          clientX: startX - 80,
          clientY: startY,
        })
      );
    });
    await expect($('[data-testid="tool-call-detail-resize-overlay"]')).not.toExist();
    await browser.execute(() => {
      const handle = document.querySelector('[data-testid="tool-call-detail-resize"]');
      if (handle instanceof HTMLElement) handle.focus();
    });
    const widthBeforeKeyboard = await browser.execute(
      () =>
        document.querySelector('[data-testid="tool-call-detail"]')?.getBoundingClientRect().width ??
        0
    );
    await browser.action('key').down('\uE012').up('\uE012').perform();
    await browser.waitUntil(
      () =>
        browser.execute(
          (width) =>
            (document.querySelector('[data-testid="tool-call-detail"]')?.getBoundingClientRect()
              .width ?? 0) > width,
          widthBeforeKeyboard
        ),
      { timeoutMsg: 'tool-call detail panel did not respond to keyboard resizing' }
    );
    const detailInteractionState = await browser.execute(() => {
      /** @param {string} selector */
      const state = (selector) => {
        const element = document.querySelector(selector);
        if (!(element instanceof HTMLElement)) return { exists: false };
        const style = getComputedStyle(element);
        const rect = element.getBoundingClientRect();
        return {
          exists: true,
          display: style.display,
          visibility: style.visibility,
          opacity: style.opacity,
          width: rect.width,
          height: rect.height,
        };
      };
      return {
        input: state('[data-testid="tool-call-copy-input"]'),
        output: state('[data-testid="tool-call-copy-output"]'),
        copyLayout: ['input', 'output'].map((target) => {
          const content = document.querySelector(`[data-testid="tool-call-${target}-json"]`);
          const button = document.querySelector(`[data-testid="tool-call-copy-${target}"]`);
          if (!(content instanceof HTMLElement) || !(button instanceof HTMLElement)) return null;
          const contentRect = content.getBoundingClientRect();
          const buttonRect = button.getBoundingClientRect();
          return {
            topInset: buttonRect.top - contentRect.top,
            rightInset: contentRect.right - buttonRect.right,
            contentPaddingTop: Number.parseFloat(getComputedStyle(content).paddingTop),
          };
        }),
      };
    });
    expect(detailInteractionState.input.exists).toBe(true);
    expect(detailInteractionState.input.width).toBeGreaterThan(0);
    expect(detailInteractionState.input.height).toBeGreaterThan(0);
    expect(detailInteractionState.output.exists).toBe(true);
    expect(detailInteractionState.output.width).toBeGreaterThan(0);
    expect(detailInteractionState.output.height).toBeGreaterThan(0);
    for (const layout of detailInteractionState.copyLayout) {
      if (!layout) throw new Error('tool-call copy button layout was unavailable');
      expect(layout.topInset).toBeLessThanOrEqual(10);
      expect(layout.rightInset).toBeLessThanOrEqual(18);
      expect(layout.contentPaddingTop).toBe(12);
    }
    await clickSelector('[data-testid="tool-call-copy-input"]');
    await browser.waitUntil(
      () =>
        browser.execute(
          () =>
            document
              .querySelector('[data-testid="tool-call-copy-input"]')
              ?.getAttribute('data-copied') === 'true'
        ),
      { timeoutMsg: 'tool-call input was not copied' }
    );
    await clickSelector('[data-testid="tool-call-view-session"]');
    await expect($(sessionsButton)).toHaveAttribute('aria-current', 'page');
    await expect($('[data-testid="session-event-e2e-event-call-3"]')).toHaveAttribute(
      'data-targeted',
      'true'
    );
    await expect($('body')).not.toHaveText(expect.stringContaining('500 Internal Error'));
  });
});
