// The search, as the panels need it.
//
// A trace is a list of events in the order they happened, which is the right
// shape to write and the wrong shape to draw. Everything here turns that list
// into the state it implies *at one point in it* — which nodes exist, which
// are closed and how, which cuts are visible where — so that a scrubber over
// the events is a scrubber over the search.
//
// The rule the whole file obeys: nothing is invented. A field a panel wants
// and the trace does not carry is absent, and the panel says so. There is no
// interpolation, no smoothing, and no derived quantity that the run did not
// produce. That is the difference between a situation room and a screensaver.

import type { Trace, TraceEvent } from "./hc";

/** How a node ended up, or that it has not. */
export type NodeStatus =
  | "open"
  | "branched"
  | "pruned"
  | "infeasible"
  | "integral"
  | "certified"
  | "blocked";

/** One node of the search tree, as far as the events have said. */
export interface TreeNode {
  id: number;
  parent: number | null;
  depth: number;
  status: NodeStatus;
  /** The event index the node was opened at. */
  opened: number;
  /** The event index the node's status last changed at. */
  settled?: number;
  /** Simplex iterations spent on this node's relaxation. A count, not a claim. */
  iterations?: number;
  /** The relaxation's objective. Floating point, from the search. */
  objective?: number;
  /** An exact bound, where one was cited. A string, always. */
  bound?: string;
  /** The exact objective of an integral relaxation. */
  integral?: string;
  /** The split this node was divided by. */
  split?: { variable: number; at: string };
  children: number[];
  /** Why an exact bound could not be given during certification. */
  blocked?: string;
}

/** A cut, and the subtree it may be used in. */
export interface Cut {
  identifier: number;
  scope: number;
  family: string;
  /** How far the relaxation's point violated it. Floating point. */
  violation: number;
  /** The event index it entered the pool at. */
  added: number;
}

/** One reading of the float bounds the search was working between. */
export interface GapReading {
  at: number;
  lower: number;
  upper: number;
}

/** A point that survived exact screening and became the incumbent. */
export interface IncumbentReading {
  at: number;
  objective: string;
  source: string;
}

/** What certification concluded, in exact quantities. */
export interface Closure {
  lower: string;
  upper: string;
  gap: string;
}

/** The state of the run as of one event. */
export interface View {
  nodes: Map<number, TreeNode>;
  order: number[];
  cuts: Cut[];
  gaps: GapReading[];
  incumbents: IncumbentReading[];
  transforms: number;
  escalations: string[];
  closure: Closure | null;
  certifying: boolean;
  status: string | null;
  root: { iterations: number; objective: number } | null;
  /** The verifier's answer, once the events carry it. */
  verdict: { accepted: boolean; rejection: string | null; work: number } | null;
  proof: { bytes: number; checksum: number } | null;
}

/**
 * A field the schema encodes as a position in a closed list.
 *
 * Those fields arrive here already spelled out: the textual encoding writes
 * the word and the binary one writes the position, and the reader that turned
 * the bytes into this document did the lookup. A second lookup on this side
 * would be a second copy of four word lists to keep in step with a frozen
 * schema, which is exactly the kind of duplication that goes stale quietly.
 */
function word(value: string | undefined): string {
  return value ?? "?";
}

/**
 * Replays a trace up to and including one event and returns the state it
 * leaves behind.
 *
 * Recomputed from the start every time rather than stepped forward, because a
 * scrubber goes backwards as often as forwards and an incrementally updated
 * model that can only go one way is a model with two implementations of every
 * transition. These traces are thousands of events at most.
 */
export function viewAt(trace: Trace | undefined, upto: number): View {
  const view: View = {
    nodes: new Map(),
    order: [],
    cuts: [],
    gaps: [],
    incumbents: [],
    transforms: 0,
    escalations: [],
    closure: null,
    certifying: false,
    status: null,
    root: null,
    verdict: null,
    proof: null,
  };
  if (!trace) return view;

  const touch = (id: number, at: number): TreeNode | undefined => {
    const node = view.nodes.get(id);
    if (node) node.settled = at;
    return node;
  };

  trace.events.slice(0, upto + 1).forEach((event: TraceEvent, at: number) => {
    switch (event.kind) {
      case "transform_applied":
        view.transforms += 1;
        break;
      case "root_lp":
        view.root = { iterations: event.iterations ?? 0, objective: Number(event.objective ?? 0) };
        break;
      case "node_open": {
        const id = event.node ?? 0;
        const parent = event.parent === undefined || event.parent === id ? null : event.parent;
        view.nodes.set(id, {
          id,
          parent,
          depth: event.depth ?? 0,
          status: "open",
          opened: at,
          children: [],
        });
        view.order.push(id);
        if (parent !== null) view.nodes.get(parent)?.children.push(id);
        break;
      }
      case "node_solved": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          if (event.iterations !== undefined) node.iterations = event.iterations;
          if (typeof event.objective === "number") node.objective = event.objective;
        }
        break;
      }
      case "node_pruned": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          node.status = "pruned";
          if (event.bound !== undefined) node.bound = event.bound;
        }
        break;
      }
      case "node_infeasible": {
        const node = touch(event.node ?? 0, at);
        if (node) node.status = "infeasible";
        break;
      }
      case "node_integral": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          node.status = "integral";
          if (typeof event.objective === "string") node.integral = event.objective;
        }
        break;
      }
      case "branch": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          node.status = "branched";
          node.split = { variable: event.variable ?? 0, at: event.split ?? "?" };
        }
        break;
      }
      case "cut_added":
        view.cuts.push({
          identifier: event.identifier ?? 0,
          scope: event.scope ?? 0,
          family: word(event.family),
          violation: event.violation ?? 0,
          added: at,
        });
        break;
      case "incumbent":
        view.incumbents.push({
          at,
          objective: String(event.objective ?? ""),
          source: word(event.source),
        });
        break;
      case "search_gap":
        view.gaps.push({ at, lower: Number(event.lower), upper: Number(event.upper) });
        break;
      case "certify_start":
        view.certifying = true;
        break;
      case "certify_node": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          node.status = "certified";
          if (event.bound !== undefined) node.bound = event.bound;
        }
        break;
      }
      case "certify_blocked": {
        const node = touch(event.node ?? 0, at);
        if (node) {
          node.status = "blocked";
          node.blocked = word(event.reason);
        }
        break;
      }
      case "certify_complete":
        view.closure = {
          lower: String(event.lower ?? ""),
          upper: String(event.upper ?? ""),
          gap: String(event.gap ?? ""),
        };
        break;
      case "escalation":
        view.escalations.push(word(event.stage));
        break;
      case "proof_assembled":
        view.proof = { bytes: event.bytes ?? 0, checksum: event.checksum ?? 0 };
        break;
      case "verify_result":
        view.verdict = {
          accepted: event.accepted === true,
          rejection: event.rejection ?? null,
          work: event.work ?? 0,
        };
        break;
      case "terminal":
        view.status = String(event.status ?? "");
        break;
      default:
        // The event list is closed and a reader that skipped an unknown kind
        // would be drawing a search that did not happen. Nothing is skipped
        // silently: an unknown kind is a schema this page was not built for,
        // and the header says which schema it expects.
        break;
    }
  });
  return view;
}

/**
 * Every node the scope of a cut reaches: the node it was derived at and
 * everything below it.
 *
 * The one invariant a visitor should be able to check by looking. A cut
 * derived at a node is sound only inside that node's subtree, so the panel
 * shades exactly that subtree and nothing else — and a sibling that lit up
 * would be the bug this project's cut discipline exists to prevent.
 */
export function scopeOf(view: View, scope: number): Set<number> {
  const inside = new Set<number>();
  const walk = (id: number) => {
    if (inside.has(id)) return;
    inside.add(id);
    for (const child of view.nodes.get(id)?.children ?? []) walk(child);
  };
  if (view.nodes.has(scope)) walk(scope);
  return inside;
}
