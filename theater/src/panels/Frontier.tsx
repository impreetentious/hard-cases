// Pause and certify: the panel that makes a partial answer a real one.
//
// A search stopped early usually leaves a number nobody can stand behind. Here
// it does not. Every node still open is either closed or given an exact lower
// bound, the smallest of those bounds is the global one, and the certificate
// carries the whole arrangement to a verifier that has never seen the search.
// What comes back is a proved interval: this is the best point found, this is
// a bound nothing feasible beats, and this is the exact distance between them.
//
// The node budget is the pause. Lower it and the interval widens; raise it and
// the interval closes, and at some budget the two ends meet and the claim
// becomes optimality. Nothing else about the run changes, which is what makes
// the panel worth playing with.

import type { Outline, ProofRecord } from "../hc";
import type { View } from "../state";
import { count, exact, ratio, search } from "../format";
import { CEILING } from "../share";

interface Props {
  view: View;
  outline: Outline | null;
  nodes: number;
  onNodes: (nodes: number) => void;
  onCertify: () => void;
  busy: boolean;
}

/** The open leaves the certificate carries, each with the bound it cites. */
function frontierOf(outline: Outline | null): ProofRecord[] {
  return (outline?.steps ?? []).filter((step) => step.kind === "leaf_frontier");
}

export default function Frontier({ view, outline, nodes, onNodes, onCertify, busy }: Props) {
  const open = frontierOf(outline);
  const blocked = [...view.nodes.values()].filter((node) => node.status === "blocked");
  const accepted = outline?.accepted === true;
  const percent = accepted ? ratio(outline?.gap) : null;

  return (
    <div className="panel split">
      <div style={{ display: "grid", gap: "20px" }}>
        <div className="card">
          <header>
            <h2>Pause &amp; certify</h2>
            <span className="note">the budget is the pause</span>
          </header>
          <div className="body">
            <p className="prose">
              Stop the search after a fixed number of nodes and prove what is known
              anyway. Every open node is given an exact lower bound or declared
              unbounded from below, and the certificate that comes out is checked
              here by the independent verifier like any other.
            </p>
            <div
              style={{
                display: "flex",
                gap: "12px",
                alignItems: "center",
                marginTop: "16px",
                flexWrap: "wrap",
              }}
            >
              <label>
                node budget
                <input
                  type="number"
                  min={1}
                  max={CEILING}
                  value={nodes}
                  data-role="budget"
                  onChange={(event) =>
                    onNodes(Math.max(1, Math.min(CEILING, Number(event.target.value) || 1)))
                  }
                />
              </label>
              <button
                type="button"
                className="act"
                data-role="certify"
                disabled={busy}
                onClick={onCertify}
              >
                pause &amp; certify
              </button>
            </div>
          </div>
        </div>

        <div className="card">
          <header>
            <h2>What was proved</h2>
            {outline && (
              <span className={`note ${accepted ? "" : "num bad"}`}>
                {accepted ? "verifier accepted" : (outline.rejection ?? "not verified")}
              </span>
            )}
          </header>
          <div className="body">
            {outline ? (
              <dl className="facts">
                <dt>claim</dt>
                <dd>{outline.claim ?? "—"}</dd>
                <dt>lower bound</dt>
                <dd className="num exact" data-role="verified-lower">
                  {exact(outline.lower)}
                </dd>
                <dt>best point</dt>
                <dd className="num exact" data-role="verified-upper">
                  {exact(outline.objective)}
                </dd>
                <dt>exact gap</dt>
                <dd className="num exact" data-role="verified-gap">
                  {exact(outline.gap)}
                  {percent ? ` (${percent})` : ""}
                </dd>
                <dt>verifier work</dt>
                <dd>{count(outline.work)} units</dd>
                <dt>widest rational</dt>
                <dd>{count(outline.widest_bits)} bits</dd>
              </dl>
            ) : (
              <p className="prose">No certificate has been checked yet.</p>
            )}
          </div>
        </div>
      </div>

      <div style={{ display: "grid", gap: "20px" }}>
        <div className="card">
          <header>
            <h2>Open frontier</h2>
            <span className="note">
              {open.length} node{open.length === 1 ? "" : "s"} left open
            </span>
          </header>
          <div className="body flush">
            {open.length === 0 ? (
              <div className="body">
                <p className="prose" style={{ margin: 0 }}>
                  {outline
                    ? "Nothing was left open: the tree closed and the claim stands on its own."
                    : "Solve something first."}
                </p>
              </div>
            ) : (
              <div className="scroller">
                <table>
                  <thead>
                    <tr>
                      <th>node</th>
                      <th>record</th>
                      <th>cites</th>
                      <th className="right">bytes</th>
                    </tr>
                  </thead>
                  <tbody>
                    {open.map((step) => (
                      <tr key={step.id} data-frontier={step.node}>
                        <td className="num exact">{step.node}</td>
                        <td>#{step.id}</td>
                        <td>bound #{step.cites}</td>
                        <td className="right">{step.length}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>

        <div className="card">
          <header>
            <h2>Preventing certification</h2>
          </header>
          <div className="body">
            {blocked.length === 0 ? (
              <p className="prose" style={{ margin: 0 }}>
                Nothing. Every open node took an exact bound.
              </p>
            ) : (
              <div className="list">
                {blocked.map((node) => (
                  <div className="entry" key={node.id} aria-selected={false}>
                    <span className="name">node {node.id}</span>
                    <span className="pill no">{node.blocked}</span>
                    <span className="why">
                      relaxation at <span className="num float">{search(node.objective)}</span>,
                      depth {node.depth}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
