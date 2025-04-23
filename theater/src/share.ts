// The address bar as the whole of the page's state.
//
// A share link that opened the page on a different instance, or on the first
// event of a run the sender was thirty events into, is a share link that
// shares nothing. Everything the panels are looking at is in the fragment, so
// copying the address is copying the view.
//
// The fragment rather than the query string, because this page is served as a
// static file and the fragment is the half a static host never sees.

/**
 * The panels the address may name.
 *
 * The address is a contract with every link anybody has already copied, so a
 * `p=` this version does not have is a link from a different one rather than a
 * panel to go looking for. Reading it as the start panel keeps the tab strip
 * and the stage saying the same thing: the render ladder ends in the case
 * files, so an unrecognised name used to land there with no tab selected and
 * with the stage's `aria-labelledby` naming a tab that is not on the page.
 *
 * This is the list, and `App` takes its tab strip from it, so a panel cannot
 * be added to one and forgotten in the other.
 */
export const PANELS = [
  "tree",
  "convergence",
  "frontier",
  "glass",
  "certificate",
  "tamper",
  "cases",
] as const;

/** One of the panels the address may name. */
export type Panel = (typeof PANELS)[number];

function isPanel(value: string): value is Panel {
  return (PANELS as readonly string[]).includes(value);
}

/** Everything the address carries. */
export interface Where {
  /** The instance file the picker is on. */
  instance: string;
  /** The node budget the last solve was run under. */
  nodes: number;
  /** Which panel is showing. */
  panel: Panel;
  /** How far along the trace the scrubber is, or -1 for the end. */
  event: number;
  /** Which certificate record the viewer has selected, or -1 for none. */
  record: number;
}

/**
 * The largest node budget this page will accept.
 *
 * The solve runs on the tab's own thread, so the budget is the only thing
 * standing between a visitor and a frozen window. The command line's ceiling
 * is ten times this and is right there, because a terminal that takes a while
 * is a terminal that takes a while. Every committed instance closes inside a
 * few dozen nodes; this is the number at which a page stops being a page.
 */
export const CEILING = 5_000;

export const START: Where = {
  instance: "fixcharge6.mps",
  nodes: 200,
  panel: "tree",
  event: -1,
  record: -1,
};

const KEYS: Record<keyof Where, string> = {
  instance: "i",
  nodes: "n",
  panel: "p",
  event: "e",
  record: "r",
};

/** Reads the address, falling back to the start for anything malformed. */
export function read(fragment: string): Where {
  const parts = new URLSearchParams(fragment.replace(/^#/, ""));
  const whole = (key: string, fallback: number) => {
    const raw = parts.get(key);
    if (raw === null) return fallback;
    const value = Number.parseInt(raw, 10);
    return Number.isFinite(value) ? value : fallback;
  };
  const panel = parts.get(KEYS.panel);
  return {
    instance: parts.get(KEYS.instance) ?? START.instance,
    nodes: Math.max(1, Math.min(CEILING, whole(KEYS.nodes, START.nodes))),
    panel: panel !== null && isPanel(panel) ? panel : START.panel,
    event: whole(KEYS.event, START.event),
    record: whole(KEYS.record, START.record),
  };
}

/** Writes the address, leaving out everything that is already the default. */
export function write(where: Where): string {
  const parts = new URLSearchParams();
  if (where.instance !== START.instance) parts.set(KEYS.instance, where.instance);
  if (where.nodes !== START.nodes) parts.set(KEYS.nodes, String(where.nodes));
  if (where.panel !== START.panel) parts.set(KEYS.panel, where.panel);
  if (where.event !== START.event) parts.set(KEYS.event, String(where.event));
  if (where.record !== START.record) parts.set(KEYS.record, String(where.record));
  const text = parts.toString();
  return text ? `#${text}` : "";
}
