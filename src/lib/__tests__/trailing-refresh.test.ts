import { afterEach, describe, expect, it, vi } from 'vitest';
import { createTrailingRefresh } from '$lib/trailing-refresh';

afterEach(() => {
  vi.useRealTimers();
});

describe('createTrailingRefresh', () => {
  it('merges repeated update events into one trailing refresh', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => {});
    const controller = createTrailingRefresh(refresh, 800);

    controller.request();
    await vi.advanceTimersByTimeAsync(400);
    controller.request();
    await vi.advanceTimersByTimeAsync(799);
    expect(refresh).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('serializes refreshes and runs one trailing update after an active query', async () => {
    vi.useFakeTimers();
    let finishFirst: (() => void) | undefined;
    const refresh = vi
      .fn<() => Promise<void>>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            finishFirst = resolve;
          })
      )
      .mockResolvedValue(undefined);
    const controller = createTrailingRefresh(refresh, 800);

    controller.request(true);
    controller.request(true);
    expect(refresh).toHaveBeenCalledTimes(1);

    finishFirst?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('merges update events during an active query into one delayed refresh', async () => {
    vi.useFakeTimers();
    let finishFirst: (() => void) | undefined;
    const refresh = vi
      .fn<() => Promise<void>>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            finishFirst = resolve;
          })
      )
      .mockResolvedValue(undefined);
    const controller = createTrailingRefresh(refresh, 800);

    controller.request(true);
    controller.request();
    controller.request();
    await vi.advanceTimersByTimeAsync(800);
    expect(refresh).toHaveBeenCalledTimes(1);

    finishFirst?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(799);
    expect(refresh).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('merges a scheduled update into an immediate follow-up refresh', async () => {
    vi.useFakeTimers();
    let finishFirst: (() => void) | undefined;
    const refresh = vi
      .fn<() => Promise<void>>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            finishFirst = resolve;
          })
      )
      .mockResolvedValue(undefined);
    const controller = createTrailingRefresh(refresh, 800);

    controller.request(true);
    controller.request(true);
    controller.request();
    finishFirst?.();
    await Promise.resolve();
    await Promise.resolve();
    expect(refresh).toHaveBeenCalledTimes(2);

    await vi.advanceTimersByTimeAsync(800);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('cancels scheduled work when its component is disposed', async () => {
    vi.useFakeTimers();
    const refresh = vi.fn(async () => {});
    const controller = createTrailingRefresh(refresh, 800);

    controller.request();
    controller.cancel();
    await vi.advanceTimersByTimeAsync(800);

    expect(refresh).not.toHaveBeenCalled();
  });
});
