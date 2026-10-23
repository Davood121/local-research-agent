const oneDecimal = new Intl.NumberFormat("en-US", {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});

/** Status-bar GB values, always one decimal so the bar does not jitter. */
export function formatGb(value: number): string {
  return oneDecimal.format(Number.isFinite(value) ? value : 0);
}
