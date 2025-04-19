// The search tree, as far as the events have said.
//
// Two things are worth looking at here and neither is the shape. The first is
// how a node was closed — by a bound, by infeasibility, by an integral point,
// or not yet — because that is the vocabulary the certificate is written in.
// The second is a cut's scope: selecting one shades the subtree it may be used
// in, and that shading is the invariant a reader can check by eye. A cut
// derived at a node is sound only below that node, and a sibling lighting up
// would be the failure the whole cut discipline exists to prevent.

import { useMemo } from "react";

import type { Cut, TreeNode, View } from "../state";
import { scopeOf } from "../state";
import { exact, search } from "../format";

const ROW = 62;
const COLUMN = 34;
const PAD = 28;

interface Placed {
  node: TreeNode;
  x: number;
  y: number;
}

/**
 * Places every node: depth downwards, and across by leaves.
 *
 * A leaf takes the next free column and a parent sits over the middle of its
 * children, which is the smallest layout that never crosses an edge. Nothing
 * cleverer is warranted — the trees this page draws are the ones a bounded
 * solve produced, and a thousand-node tree is unreadable at any layout.
 */
function place(view: View): { placed: Placed[]; width: number; height: number } {
  const placed: Placed[] = [];
  let column = 0;
  let deepest = 0;

  const walk = (id: number): number => {
    const node = view.nodes.get(id);
    if (!node) return column;
    deepest = Math.max(deepest, node.depth);
    const children = node.children.filter((child) => view.nodes.has(child));
    let x: number;
    if (children.length === 0) {
      x = column;
      column += 1;
    } else {
      const spots = children.map(walk);
      x = ((spots[0] ?? column) + (spots[spots.length - 1] ?? column)) / 2;
    }
    placed.push({ node, x: PAD + x * COLUMN, y: PAD + node.depth * ROW });
    return x;
  };

  for (const id of view.order) {
    if (view.nodes.get(id)?.parent === null) walk(id);
  }
  return {
    placed,
    width: PAD * 2 + Math.max(1, column) * COLUMN,
    height: PAD * 2 + (deepest + 1) * ROW,
  };
}

interface Props {
  view: View;
  chosen: number | null;
  onChoose: (id: number | null) => void;
  cut: Cut | null;
  onCut: (cut: Cut | null) => void;
  /**
   * The reduced model's column labels, empty when the run carried no chain.
   *
   * A split cites a column of the model the search was about. Naming it from
   * the original file would be a guess, because presolve removes columns, so
   * an absent label falls back to the index the record actually carries.
   */
  columns: string[];
  /** How the page's contents were produced, which decides whether there are labels. */
  provenance: "live" | "replay" | null;
}

export default function Tree({
  view,
  chosen,
  onChoose,
  cut,
  onCut,
  columns,
  provenance,
}: Props) {
  const { placed, width, height } = useMemo(() => place(view), [view]);
  const inScope = useMemo(
    () => (cut ? scopeOf(view, cut.scope) : new Set<number>()),
    [view, cut],
  );
  const spot = useMemo(() => {
    const map = new Map<number, Placed>();
    for (const item of placed) map.set(item.node.id, item);
    return map;
  }, [placed]);

  if (placed.length === 0) {
    return (
      <div className="empty">
        No node has been opened yet. Solve an instance, or scrub forward until the
        search reaches one.
      </div>
    );
  }

  const node = chosen === null ? null : view.nodes.get(chosen) ?? null;

  return (
    <div className="panel split">
      <div className="card">
        <header>
          <h2>Tree</h2>
          <span className="note">
            {placed.length} node{placed.length === 1 ? "" : "s"} ·{" "}
            {view.cuts.length} cut{view.cuts.length === 1 ? "" : "s"}
          </span>
        </header>
        <div className="body flush">
          <div className="tree-frame">
            <svg width={width} height={height} role="img" aria-label="the search tree">
              {placed.map(({ node: child }) => {
                if (child.parent === null) return null;
                const from = spot.get(child.parent);
                const to = spot.get(child.id);
                if (!from || !to) return null;
                return (
                  <path
                    key={`edge-${child.id}`}
                    className="tree-edge"
                    d={`M ${from.x} ${from.y + 9} C ${from.x} ${from.y + ROW / 2}, ${to.x} ${
                      to.y - ROW / 2
                    }, ${to.x} ${to.y - 9}`}
                  />
                );
              })}
              {placed.map(({ node: item, x, y }) => (
                <g
                  key={item.id}
                  role="button"
                  tabIndex={0}
                  aria-pressed={chosen === item.id}
                  aria-label={`node ${item.id}, ${item.status}`}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      onChoose(chosen === item.id ? null : item.id);
                    }
                  }}
                  className={[
                    "tree-node",
                    `st-${item.status}`,
                    chosen === item.id ? "chosen" : "",
                    inScope.has(item.id) ? "in-scope" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  data-node={item.id}
                  data-status={item.status}
                  data-in-scope={inScope.has(item.id) ? "yes" : "no"}
                  onClick={() => onChoose(chosen === item.id ? null : item.id)}
                >
                  <circle cx={x} cy={y} r={9} />
                  <text
                    x={x}
                    y={y + 21}
                    textAnchor="middle"
                    className="axis-text"
                  >
                    {item.id}
                  </text>
                  {view.cuts.some((entry) => entry.scope === item.id) && (
                    <text x={x + 12} y={y - 8} className="tree-cut">
                      ✂
                    </text>
                  )}
                </g>
              ))}
            </svg>
          </div>
        </div>
      </div>

      <div style={{ display: "grid", gap: "20px" }}>
        <div className="card">
          <header>
            <h2>Node</h2>
          </header>
          <div className="body">
            {node ? (
              <dl className="facts">
                <dt>identifier</dt>
                <dd>{node.id}</dd>
                <dt>parent</dt>
                <dd>{node.parent === null ? "root" : node.parent}</dd>
                <dt>depth</dt>
                <dd>{node.depth}</dd>
                <dt>status</dt>
                <dd>{node.status}</dd>
                <dt>iterations</dt>
                <dd>{node.iterations ?? "—"}</dd>
                <dt>relaxation</dt>
                <dd className="num float">{search(node.objective)}</dd>
                <dt>exact bound</dt>
                <dd className="num exact">{exact(node.bound)}</dd>
                {node.integral && (
                  <>
                    <dt>integral at</dt>
                    <dd className="num exact">{exact(node.integral)}</dd>
                  </>
                )}
                {node.split && (
                  <>
                    <dt>split</dt>
                    <dd className="num exact">
                      {columns[node.split.variable] ?? `column ${node.split.variable}`} at{" "}
                      {node.split.at}
                    </dd>
                  </>
                )}
                {node.blocked && (
                  <>
                    <dt>blocked by</dt>
                    <dd className="num bad">{node.blocked}</dd>
                  </>
                )}
              </dl>
            ) : (
              <p className="prose">Select a node to see what the run recorded about it.</p>
            )}
            {node?.split && columns.length === 0 && (
              <p className="prose" style={{ marginBottom: 0 }}>
                The split names a column by its index because{" "}
                {provenance === "replay"
                  ? "a recording carries a trace and a certificate and no reduction chain"
                  : "this run carried no reduction chain"}
                , and a split cites a column of the model the search was about.
                Labelling it from the file would be a guess: presolve removes
                columns, so the two coordinate systems are not the same one.
              </p>
            )}
          </div>
        </div>

        <div className="card">
          <header>
            <h2>Cut pool</h2>
            <span className="note">select to shade the subtree</span>
          </header>
          <div className="body">
            {view.cuts.length === 0 ? (
              <p className="prose">This run separated nothing.</p>
            ) : (
              <div className="list">
                {view.cuts.map((entry) => (
                  <button
                    type="button"
                    key={entry.identifier}
                    className="entry"
                    data-cut={entry.identifier}
                    aria-selected={cut?.identifier === entry.identifier}
                    onClick={() =>
                      onCut(cut?.identifier === entry.identifier ? null : entry)
                    }
                  >
                    <span className="name">
                      #{entry.identifier} {entry.family}
                    </span>
                    <span className="pill">scope {entry.scope}</span>
                    <span className="why">
                      violated by <span className="num float">{search(entry.violation)}</span>{" "}
                      at the point that derived it
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
