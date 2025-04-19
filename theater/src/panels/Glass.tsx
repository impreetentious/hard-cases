// Simplex glass: the root relaxation, pivot by pivot.
//
// Under the textbook rule rather than the one the solver runs by default, and
// that is stated at the top of the panel rather than hidden. Devex chooses by
// a reference framework nobody can see, so watching it is watching a number
// change; Dantzig chooses the most negative reduced cost, which is a sentence
// a reader can follow. The same code path runs either way — the search is
// literally the ordinary one with a trail attached — so what is shown here is
// what a simplex does, not a teaching version of it.
//
// Every column in this table is floating point and none of it is evidence.
// That is why there is not a single exact quantity on the panel: an exact
// number in a window about the search would be the one place this page could
// mislead somebody.

import type { Glass as GlassReport } from "../hc";
import { count, search } from "../format";

interface Props {
  glass: GlassReport | null;
  /** How the rest of the page was produced, which this panel is not. */
  provenance: "live" | "replay" | null;
}

export default function Glass({ glass, provenance }: Props) {
  if (!glass) {
    return <div className="empty">Solve an instance and the pivots appear here.</div>;
  }
  if (glass.refused) {
    return <div className="empty">{glass.refused}</div>;
  }

  const names = glass.columns;
  const nameOf = (index: number | null | undefined) =>
    index === null || index === undefined ? "—" : (names[index]?.name ?? `col ${index}`);
  const widest = glass.pivots.reduce(
    (most, pivot) => Math.max(most, Math.abs(pivot.objective ?? 0)),
    1,
  );

  return (
    <div className="panel">
      <div className="card">
        <header>
          <h2>Simplex glass</h2>
          <span className="note">
            {glass.rule} rule · {count(glass.iterations)} iteration
            {glass.iterations === 1 ? "" : "s"} · ended {glass.outcome}
          </span>
        </header>
        <div className="body">
          <dl className="facts" style={{ gridTemplateColumns: "repeat(4, auto 1fr)" }}>
            <dt>bound flips</dt>
            <dd>{count(glass.flips)}</dd>
            <dt>degenerate</dt>
            <dd>{count(glass.degenerate)}</dd>
            <dt>refactorizations</dt>
            <dd>{count(glass.refactorizations)}</dd>
            <dt>escalations</dt>
            <dd>{count(glass.escalations)}</dd>
          </dl>
          <p className="prose">
            This panel is the one that is always <strong>run here, now</strong>
            {provenance === "replay"
              ? ", including while the rest of the page is showing a recording — a trace records which nodes were opened and not which columns entered a basis, so there is nothing here to replay."
              : "."}{" "}
            It solves <strong>the file's own relaxation</strong> rather than the
            reduced model the tree search works on, because presolve renames and
            removes the columns this table exists to name; the two are the same
            problem only when presolve found nothing to do. The step lengths are
            in the search's scaled coordinates. The objective is not: scaling
            divides a column and multiplies its cost by the same amount.
          </p>
          {glass.truncated && (
            <p className="prose" style={{ marginBottom: 0 }}>
              This run made {count(glass.iterations)} iterations and the panel keeps{" "}
              {count(glass.kept)}. The rest happened and are not shown; a window that
              renamed the first few hundred as the whole run would be lying about the
              size of the problem.
            </p>
          )}
        </div>
      </div>

      <div className="card">
        <header>
          <h2>Iterations</h2>
          <span className="note">every column here is floating point</span>
        </header>
        <div className="body flush">
          {glass.pivots.length === 0 ? (
            <div className="body">
              <p className="prose" style={{ margin: 0 }}>
                The starting basis was already optimal: nothing priced better and no
                pivot was made.
              </p>
            </div>
          ) : (
            <div className="scroller">
              <table>
                <thead>
                  <tr>
                    <th className="right">#</th>
                    <th>phase</th>
                    <th>entering</th>
                    <th>leaving</th>
                    <th className="right">step</th>
                    <th className="right">objective</th>
                    <th className="right">infeasibility</th>
                    <th>rung</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {glass.pivots.map((pivot) => (
                    <tr key={pivot.iteration} data-pivot={pivot.iteration}>
                      <td className="right">{pivot.iteration}</td>
                      <td>{pivot.phase === 1 ? "feasibility" : "objective"}</td>
                      <td>
                        {nameOf(pivot.entering)}{" "}
                        <span className="num float">{pivot.direction > 0 ? "↑" : "↓"}</span>
                      </td>
                      <td>{pivot.leaving === null ? "bound flip" : nameOf(pivot.leaving)}</td>
                      <td className="right num float">{search(pivot.theta)}</td>
                      <td className="right num float">{search(pivot.objective)}</td>
                      <td className="right num float">{search(pivot.infeasibility)}</td>
                      <td>{pivot.rung}</td>
                      <td style={{ width: "22%" }}>
                        <span
                          className="bar"
                          style={{
                            width: `${
                              (Math.abs(pivot.objective ?? 0) / widest) * 100
                            }%`,
                          }}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
