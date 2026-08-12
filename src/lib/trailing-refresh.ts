export interface TrailingRefresh {
  request(immediate?: boolean): void;
  cancel(): void;
}

export function createTrailingRefresh(
  refresh: () => Promise<void>,
  delayMs = 800
): TrailingRefresh {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let pending = false;
  let pendingImmediate = false;
  let disposed = false;

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void run();
    }, delayMs);
  }

  async function run() {
    if (disposed) return;
    if (running) {
      pending = true;
      return;
    }

    running = true;
    try {
      await refresh();
    } finally {
      running = false;
      if (pendingImmediate && !disposed) {
        clearTimeout(timer);
        timer = undefined;
        pendingImmediate = false;
        pending = false;
        void run();
      } else if (pending && !disposed) {
        pending = false;
        schedule();
      }
    }
  }

  return {
    request(immediate = false) {
      if (disposed) return;
      if (immediate) {
        clearTimeout(timer);
        timer = undefined;
        if (running) {
          pendingImmediate = true;
          return;
        }
        void run();
      } else {
        schedule();
      }
    },
    cancel() {
      disposed = true;
      pending = false;
      pendingImmediate = false;
      clearTimeout(timer);
      timer = undefined;
    },
  };
}
