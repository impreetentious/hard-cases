// Rendering, and the one distinction it must never blur.
//
// A quantity out of the floating-point search and a quantity a proof could
// cite are different things, and this page is about the difference. They are
// therefore formatted by different functions with different names, and the
// float one deliberately produces something that does not look exact: a fixed
// number of places and a leading tilde, so a reader who glances at a number
// can tell which kind it is without reading the label beside it.

/** A float out of the search. Never a claim, and shaped so it cannot be read as one. */
export function search(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return "—";
  const magnitude = Math.abs(value);
  if (magnitude !== 0 && (magnitude < 1e-4 || magnitude >= 1e9)) {
    return `~${value.toExponential(3)}`;
  }
  return `~${value.toFixed(4)}`;
}

/** An exact quantity. Already a string when it arrives, and kept one. */
export function exact(value: string | null | undefined): string {
  if (value === null || value === undefined || value === "") return "—";
  return value;
}

/** An exact ratio as a decimal, for the one place a reader wants a size rather than a value. */
export function ratio(value: string | null | undefined): string | null {
  if (!value) return null;
  const [top, bottom] = value.split("/");
  const above = Number(top);
  const below = bottom === undefined ? 1 : Number(bottom);
  if (!Number.isFinite(above) || !Number.isFinite(below) || below === 0) return null;
  return `${((above / below) * 100).toFixed(2)}%`;
}

/** A digest, short enough to sit in a header and long enough to compare. */
export function digest(value: string | undefined): string {
  if (!value) return "—";
  return `${value.slice(0, 8)}…${value.slice(-4)}`;
}

/** A count with a thousands separator, which is every count on this page. */
export function count(value: number | undefined): string {
  return (value ?? 0).toLocaleString("en-US");
}

/** Bytes, in the units a certificate is actually measured in. */
export function bytes(value: number | undefined): string {
  if (value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  return `${(value / 1024).toFixed(1)} KiB`;
}
