// The convergence strip: what the search believed, what the solver worked out
// exactly, and what an independent program then accepted.
//
// Three states, not two, because there are three. A relaxation objective is a
// float out of the simplex and is evidence of nothing: amber, dashed. A node
// bound the exact rebuild established is a rational number the certificate
// will cite, but nobody has checked it yet: dim cyan, square. A bound the
// standalone verifier recomputed from the certificate and the original file is
// the only thing on this page that has been checked by something that did not
// produce it: bright cyan, solid.
//
// Collapsing the middle state into either of the others is the mistake this
// panel exists to avoid. Calling it float understates what exact assembly did;
// calling it verified is the claim the whole project is against.

import { useMemo } from "react";

import type { View } from "../state";
import type { Outline, Trace } from "../hc";
import { exact, ratio, search } from "../format";

interface Props {
  view: View;
  trace: Trace | undefined;
  cursor: number;
  outline: Outline | null;
}

const WIDTH = 960;
const HEIGHT = 300;
const LEFT = 84;
const RIGHT = 20;
const TOP = 20;
const BOTTOM = 38;

/** An exact quantity as a number, for placing a mark and for nothing else. */
function asNumber(value: string | null | undefined): number | null {
  if (!value) return null;
  const [top, bottom] = value.split("/");
  const above = Number(top);
  const below = bottom === undefined ? 1 : Number(bottom);
  if (!Number.isFinite(above) || !Number.isFinite(below) || below === 0) return null;
  return above / below;
}

/** One thing worth plotting, with which of the three states it is in. */
interface Point {
  at: number;
  value: number;
  kind: "float" | "exact" | "verified";
  label: string;
}

export default function Convergence({ view, trace, cursor, outline }: Props) {
  const accepted = outline?.accepted === true;

  const points = useMemo(() => {
    const out: Point[] = [];
    const events = trace?.events.slice(0, cursor + 1) ?? [];
    events.forEach((event, at) => {
      if (event.kind === "root_lp" || event.kind === "node_solved") {
        if (typeof event.objective === "number") {
          out.push({
            at,
            value: event.objective,
            kind: "float",
            label: `node ${event.node ?? 0} relaxation`,
          });
        }
        return;
      }
      const exactValue =
        event.kind === "node_pruned" || event.kind === "certify_node"
          ? event.bound
          : event.kind === "node_integral"
            ? (event.objective as string | undefined)
            : undefined;
      if (typeof exactValue === "string") {
        const value = asNumber(exactValue);
        if (value !== null) {
          out.push({ at, value, kind: "exact", label: `node ${event.node ?? 0} bound` });
        }
      }
    });
    return out;
  }, [trace, cursor]);

  // Only what the verdict actually carries. A claim that certifies a point and
  // no bound at all — which is a real status here, and the honest one for a
  // search that could not rebuild a bound exactly — has a `lower` of null, and
  // falling back to the objective would draw a certified lower bound across a
  // page whose whole subject is not doing that.
  const certifiedLower = accepted ? asNumber(outline?.lower) : null;
  const certifiedUpper = accepted ? asNumber(outline?.objective) : null;

  const geometry = useMemo(() => {
    const values = points.map((point) => point.value);
    if (certifiedLower !== null) values.push(certifiedLower);
    if (certifiedUpper !== null) values.push(certifiedUpper);
    for (const incumbent of view.incumbents) {
      const value = asNumber(incumbent.objective);
      if (value !== null) values.push(value);
    }
    if (values.length === 0) return null;
    let low = Math.min(...values);
    let high = Math.max(...values);
    if (low === high) {
      low -= 1;
      high += 1;
    }
    const margin = (high - low) * 0.14;
    low -= margin;
    high += margin;
    const span = Math.max(1, (trace?.events.length ?? 1) - 1);
    return {
      x: (at: number) => LEFT + ((WIDTH - LEFT - RIGHT) * at) / span,
      y: (value: number) => TOP + (HEIGHT - TOP - BOTTOM) * (1 - (value - low) / (high - low)),
      low,
      high,
    };
  }, [points, view.incumbents, certifiedLower, certifiedUpper, trace]);

  if (!geometry) {
    return (
      <div className="empty">
        Nothing has reported a bound yet. Nothing is drawn from nothing.
      </div>
    );
  }

  const { x, y, low, high } = geometry;
  const floats = points.filter((point) => point.kind === "float");
  const exacts = points.filter((point) => point.kind === "exact");
  const line = floats.map((point, at) => `${at === 0 ? "M" : "L"} ${x(point.at)} ${y(point.value)}`).join(" ");
  const gapPercent = accepted ? ratio(outline?.gap) : null;

  return (
    <div className="panel split">
      <div className="card">
        <header>
          <h2>Convergence</h2>
          <span className="note">float · exact · verified</span>
        </header>
        <div className="body">
          <svg
            className="strip"
            viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
            preserveAspectRatio="none"
            role="img"
            aria-label="what the search believed against what was proved"
          >
            <line className="axis" x1={LEFT} y1={TOP} x2={LEFT} y2={HEIGHT - BOTTOM} />
            <line
              className="axis"
              x1={LEFT}
              y1={HEIGHT - BOTTOM}
              x2={WIDTH - RIGHT}
              y2={HEIGHT - BOTTOM}
            />
            {[high, (high + low) / 2, low].map((value, at) => (
              <text key={at} className="axis-text" x={8} y={y(value) + 4}>
                {search(value)}
              </text>
            ))}

            <path className="series-float" d={line} />
            {floats.map((point, at) => (
              <circle
                key={`f${at}`}
                className="mark-float"
                data-role="relaxation"
                cx={x(point.at)}
                cy={y(point.value)}
                r={2.5}
              />
            ))}

            {exacts.map((point, at) => (
              <rect
                key={`e${at}`}
                className="mark-exact"
                data-role="exact-bound"
                x={x(point.at) - 3.5}
                y={y(point.value) - 3.5}
                width={7}
                height={7}
              />
            ))}

            {certifiedLower !== null && (
              <line
                className="series-exact"
                data-role="certified-lower"
                x1={LEFT}
                y1={y(certifiedLower)}
                x2={WIDTH - RIGHT}
                y2={y(certifiedLower)}
              />
            )}
            {certifiedUpper !== null && certifiedUpper !== certifiedLower && (
              <line
                className="series-exact"
                data-role="certified-upper"
                x1={LEFT}
                y1={y(certifiedUpper)}
                x2={WIDTH - RIGHT}
                y2={y(certifiedUpper)}
              />
            )}

            {view.incumbents.map((point, at) => {
              const value = asNumber(point.objective);
              if (value === null) return null;
              return (
                <circle
                  key={`i${at}`}
                  className={accepted ? "mark-verified" : "mark-unverified"}
                  data-role="incumbent"
                  data-verified={accepted ? "yes" : "no"}
                  cx={x(point.at)}
                  cy={y(value)}
                  r={5}
                />
              );
            })}
          </svg>
          <div className="legend" style={{ marginTop: "12px" }}>
            <span>
              <i className="float-key" />
              relaxation objectives, floating point
            </span>
            <span>
              <i className="exactish-key" />
              node bounds, exact and unchecked
            </span>
            <span>
              <i className="exact-key" />
              recomputed and accepted by the verifier
            </span>
          </div>
        </div>
      </div>

      <div className="card">
        <header>
          <h2>Where it stands</h2>
        </header>
        <div className="body">
          <dl className="facts">
            <dt>root relaxation</dt>
            <dd className="num float">{search(view.root?.objective)}</dd>
            <dt>nodes solved</dt>
            <dd>{floats.length}</dd>
            <dt>exact node bounds</dt>
            <dd>{exacts.length}</dd>
            <dt>incumbent</dt>
            <dd className="num exactish">{exact(view.incumbents.at(-1)?.objective)}</dd>
            <dt>verified lower</dt>
            <dd className="num exact">{accepted ? exact(outline?.lower) : "—"}</dd>
            <dt>verified objective</dt>
            <dd className="num exact">{accepted ? exact(outline?.objective) : "—"}</dd>
            <dt>verified gap</dt>
            <dd className="num exact">
              {accepted ? exact(outline?.gap) : "—"}
              {gapPercent ? ` (${gapPercent})` : ""}
            </dd>
          </dl>
          <p className="prose" style={{ marginBottom: 0 }}>
            {accepted ? (
              <>
                The solid lines and the filled point come from the{" "}
                <strong>independent verifier</strong>, which recomputed them from the
                certificate and the original file rather than reading them off the
                run. Everything else on the strip is the search's own account of
                itself.
              </>
            ) : (
              <>
                Nothing has been verified yet, so nothing is drawn solid. The squares
                are exact rationals the solver worked out and the line is floating
                point; neither has been checked by anything that did not produce it.
              </>
            )}
          </p>
        </div>
      </div>
    </div>
  );
}
