export function normalizePageInput(value: string, pageCount: number): number | null {
  const normalizedValue = value.trim();
  if (!/^\d+$/.test(normalizedValue)) return null;

  const requestedPage = Number(normalizedValue);
  if (!Number.isSafeInteger(requestedPage)) return null;

  return Math.min(Math.max(requestedPage, 1), Math.max(pageCount, 1));
}
