// The certificate, as the verifier reads it.
//
// Not as the solver wrote it. Every record on this panel came back from the
// independent checker's own reader, and every quantity beside it was
// recomputed rather than adopted: the point is checked against the model
// before its objective is worth computing, the cut's coefficients are derived
// again from the combination it names, the transform is applied again and its
// result hashed. A viewer built on the assembler's view of its own output
// would be a very pretty way of believing the solver.
//
// The bytes are shown too, with the selected record's payload lit. A reader
// who wants to check the length arithmetic themselves can.

import { useMemo } from "react";

import type { Outline, ProofRecord } from "../hc";
import { bytes as asBytes, count, exact } from "../format";

interface Props {
  outline: Outline | null;
  hex: string | null;
  chosen: number;
  onChoose: (id: number) => void;
  /** The reduced model's column labels, empty when the run carried no chain. */
  columns: string[];
}

const GROUPS: { title: string; kinds: string[]; about: string }[] = [
  {
    title: "Transform chain",
    kinds: ["transform", "transform_contradiction"],
    about:
      "Each step is a presolve equivalence and the hash of the model it produces. The verifier applies the rule itself and hashes the result; a stated hash is never adopted.",
  },
  {
    title: "Inferences",
    kinds: ["dual_bound", "contradiction"],
    about:
      "Nonnegative combinations of inequalities visible at a node. The multipliers are checked for sign before the sum is formed, because after the sum a reversed inequality is indistinguishable from arithmetic that came out wrong.",
  },
  {
    title: "Cuts",
    kinds: ["cut"],
    about:
      "Each cut is rederived from the combination it names and is usable only inside the subtree of the node it was derived at. Both halves are checked.",
  },
  {
    title: "Scoped inference tree",
    kinds: [
      "root",
      "branch",
      "leaf_bound",
      "leaf_infeasible",
      "leaf_integral",
      "leaf_frontier",
    ],
    about:
      "A recursive cover of the feasible region rather than a bag of leaves: every node is either split into two children that between them lose nothing, or closed by something citable.",
  },
  {
    title: "Witnesses",
    kinds: ["incumbent", "feasible_point", "ray"],
    about:
      "Points and directions, given in the original file's coordinates so that a reader never has to trust a presolve map to check them.",
  },
  {
    title: "The claim",
    kinds: ["assertion", "terminator"],
    about:
      "What the whole thing asserts, and a count per record kind so that a stream missing a record is caught by arithmetic rather than by noticing.",
  },
];

/** The fields worth showing for a record, as short text. */
function detail(step: ProofRecord, columns: string[]): string {
  const parts: string[] = [];
  if (step.node !== undefined) parts.push(`node ${step.node}`);
  if (step.scope !== undefined) parts.push(`scope ${step.scope}`);
  if (step.rule !== undefined) parts.push(`rule ${step.rule}`);
  if (step.family) parts.push(step.family);
  if (step.terms !== undefined) parts.push(`${step.terms} term${step.terms === 1 ? "" : "s"}`);
  if (step.cover !== undefined) parts.push(`cover of ${step.cover}`);
  if (step.variable !== undefined) {
    parts.push(columns[step.variable] ?? `column ${step.variable}`);
  }
  if (step.split !== undefined) parts.push(`at ${step.split}`);
  if (step.left !== undefined && step.right !== undefined) {
    parts.push(`children ${step.left} and ${step.right}`);
  }
  if (step.cites !== undefined) parts.push(`cites #${step.cites}`);
  if (step.width !== undefined) parts.push(`${step.width} component${step.width === 1 ? "" : "s"}`);
  if (step.claim) parts.push(step.claim);
  if (step.stated) parts.push(`states ${step.stated}`);
  if (step.kinds !== undefined) parts.push(`${step.kinds} kinds counted`);
  return parts.join(" · ");
}

export default function Certificate({ outline, hex, chosen, onChoose, columns }: Props) {
  const selected = useMemo(
    () => (outline?.steps ?? []).find((step) => step.id === chosen) ?? null,
    [outline, chosen],
  );

  // A certificate large enough to be worth capping would be one the page could
  // not have produced under its node budget, so this is insurance rather than
  // a limit anyone will meet — and when it fires it says the size rather than
  // silently showing a prefix as though it were the whole thing.
  const TOO_LONG = 200_000;

  const marked = useMemo(() => {
    if (!hex || hex.length > TOO_LONG) return null;
    if (!selected) return [hex, "", ""] as const;
    const from = selected.at * 2;
    const to = (selected.at + selected.length) * 2;
    return [hex.slice(0, from), hex.slice(from, to), hex.slice(to)] as const;
  }, [hex, selected]);

  if (!outline) {
    return <div className="empty">Solve an instance to see its certificate.</div>;
  }
  if (!outline.read) {
    return (
      <div className="card">
        <header>
          <h2>Refused before it was read</h2>
          <span className="note num bad">{outline.rejection}</span>
        </header>
        <div className="body">
          <p className="prose" style={{ margin: 0 }}>
            {outline.detail}
          </p>
        </div>
      </div>
    );
  }

  const steps = outline.steps ?? [];

  return (
    <div className="panel split">
      <div style={{ display: "grid", gap: "20px" }}>
        {GROUPS.map((group) => {
          const inside = steps.filter((step) => group.kinds.includes(step.kind));
          if (inside.length === 0) return null;
          return (
            <div className="card" key={group.title}>
              <header>
                <h2>{group.title}</h2>
                <span className="note">{inside.length}</span>
              </header>
              <div className="body flush">
                <div className="body" style={{ paddingBottom: 0 }}>
                  <p className="prose" style={{ margin: 0 }}>
                    {group.about}
                  </p>
                </div>
                <table>
                  <thead>
                    <tr>
                      <th className="right">#</th>
                      <th>kind</th>
                      <th>detail</th>
                      <th className="right">at</th>
                      <th className="right">bytes</th>
                    </tr>
                  </thead>
                  <tbody>
                    {inside.map((step) => (
                      <tr
                        key={step.id}
                        data-record={step.id}
                        data-kind={step.kind}
                        aria-selected={chosen === step.id}
                        onClick={() => onChoose(chosen === step.id ? -1 : step.id)}
                        style={{ cursor: "pointer" }}
                      >
                        <td className="right">{step.id}</td>
                        <td>{step.kind}</td>
                        <td style={{ whiteSpace: "normal" }}>{detail(step, columns)}</td>
                        <td className="right">{step.at}</td>
                        <td className="right">{step.length}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          );
        })}
      </div>

      <div style={{ display: "grid", gap: "20px" }}>
        <div className="card">
          <header>
            <h2>Verdict</h2>
            <span className={`note ${outline.accepted ? "" : "num bad"}`}>
              {outline.accepted ? "accepted" : outline.rejection}
            </span>
          </header>
          <div className="body">
            <dl className="facts">
              <dt>claim</dt>
              <dd>{outline.claim}</dd>
              <dt>records</dt>
              <dd>{steps.length}</dd>
              <dt>certificate</dt>
              <dd>{asBytes(outline.bytes)}</dd>
              <dt>work</dt>
              <dd>{outline.accepted ? `${count(outline.work)} units` : "—"}</dd>
              <dt>widest rational</dt>
              <dd>{outline.accepted ? `${count(outline.widest_bits)} bits` : "—"}</dd>
              <dt>objective</dt>
              <dd className="num exact">{exact(outline.objective)}</dd>
              <dt>lower bound</dt>
              <dd className="num exact">{exact(outline.lower)}</dd>
              <dt>gap</dt>
              <dd className="num exact">{exact(outline.gap)}</dd>
            </dl>
            {!outline.accepted && outline.detail && (
              <p className="prose" style={{ marginBottom: 0 }}>
                {outline.detail}
              </p>
            )}
          </div>
        </div>

        <div className="card">
          <header>
            <h2>Bytes</h2>
            <span className="note">
              {selected ? `record #${selected.id} lit` : "select a record"}
            </span>
          </header>
          <div className="body">
            {marked ? (
              <div className="hex">
                {marked[0]}
                <b>{marked[1]}</b>
                {marked[2]}
              </div>
            ) : hex ? (
              <p className="prose" style={{ margin: 0 }}>
                This certificate is {(hex.length / 2).toLocaleString("en-US")} bytes,
                which is more than this panel will render at once. Every record's
                offset and length is in the table beside it, and{" "}
                <code className="num">hc explain --proof</code> lists the same
                records at the command line.
              </p>
            ) : (
              <p className="prose" style={{ margin: 0 }}>
                No certificate on the page.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
