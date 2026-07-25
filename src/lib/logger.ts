type LogLevel = 'error' | 'warn';

function formatReason(reason: unknown): string {
  if (reason instanceof Error) {
    return `${reason.name}: ${reason.message}`;
  }
  if (typeof reason === 'string') {
    return reason;
  }
  try {
    return JSON.stringify(reason);
  } catch {
    return String(reason);
  }
}

async function writeToAppLog(level: LogLevel, message: string): Promise<void> {
  if (typeof window === 'undefined') return;

  try {
    const { isTauri } = await import('@tauri-apps/api/core');
    if (!isTauri()) return;

    const logger = await import('@tauri-apps/plugin-log');
    if (level === 'error') {
      await logger.error(message);
    } else {
      await logger.warn(message);
    }
  } catch {
    // Logging must never change the outcome of the operation being reported.
  }
}

function report(level: LogLevel, context: string, reason: unknown): void {
  const message = `${context}: ${formatReason(reason)}`;
  console[level](context, reason);
  void writeToAppLog(level, message);
}

export function logError(context: string, reason: unknown): void {
  report('error', context, reason);
}

export function logWarn(context: string, reason: unknown): void {
  report('warn', context, reason);
}

export async function installFrontendErrorLogging(): Promise<() => void> {
  if (typeof window === 'undefined') return () => {};

  const { isTauri } = await import('@tauri-apps/api/core');
  if (!isTauri()) return () => {};

  const onError = (event: ErrorEvent) => {
    logError('Unhandled frontend error', event.error ?? event.message);
  };
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    logError('Unhandled frontend promise rejection', event.reason);
  };

  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onUnhandledRejection);

  return () => {
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onUnhandledRejection);
  };
}
