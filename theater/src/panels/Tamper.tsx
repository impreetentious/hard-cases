// The tamper lab.
//
// The claim this project makes is that the verifier is doing something. The
// only honest way to show that is to break a certificate in front of a reader
// and watch it get refused by name, so that is what this panel is: the whole
// corpus of named mutations the harness runs in continuous integration, driven
// here against the certificate on the page, plus a field editor for anything
// the corpus never thought of.
//
// The editor repairs the checksum itself rather than asking the module to.
// Anyone holding the format specification can repair a checksum, so a forgery
// that fails on corruption establishes nothing; the interesting forgery is the
// one that is well-formed and still wrong. The polynomial is implemented in
// this page for exactly that reason — the lab has to be able to seal a lie.
//
// An entry's expected identifier is the earliest rule that catches it on the
// harness's own models. On another model an earlier rule may see the defect
// first, and the panel says so rather than calling the difference a failure.
// What must never happen, on any model, is acceptance.

import { useMemo, useState } from "react";

import type { Outline, TamperCase, Tampered } from "../hc";

interface Props {
  cases: TamperCase[];
  outline: Outline | null;
  hex: string | null;
  result: Tampered | null;
  edited: {
    outline: Outline;
    at: number;
    from: number;
    to: number;
    sealed: boolean;
    hex: string;
  } | null;
  busy: boolean;
  onCase: (name: string) => void;
  onEdit: (at: number, value: number, reseal: boolean) => void;
}

export default function Tamper({
  cases,
  outline,
  hex,
  result,
  edited,
  busy,
  onCase,
  onEdit,
}: Props) {
  const records = outline?.steps ?? [];
  const [record, setRecord] = useState<number>(-1);
  const [offset, setOffset] = useState<number>(0);
  const [value, setValue] = useState<number>(1);
  const [reseal, setReseal] = useState<boolean>(true);

  const target = useMemo(
    () => records.find((step) => step.id === record) ?? null,
    [records, record],
  );
  const at = target ? target.at + Math.min(offset, Math.max(0, target.length - 1)) : -1;
  const current = hex && at >= 0 ? Number.parseInt(hex.slice(at * 2, at * 2 + 2), 16) : null;

  if (!outline || !hex) {
    return <div className="empty">Solve an instance first; there is nothing to forge yet.</div>;
  }

  return (
    <div className="panel split">
      <div className="card">
        <header>
          <h2>The corpus</h2>
          <span className="note">{cases.length} named mutations</span>
        </header>
        <div className="body flush">
          <div className="body" style={{ paddingBottom: 0 }}>
            <p className="prose" style={{ margin: 0 }}>
              Every one of these is a real edit to a real certificate, re-sealed so
              the checksum is correct, and handed to the verifier compiled into this
              page. Each names the rule that should answer it.
            </p>
          </div>
          <div className="body">
            <div className="list">
              {cases.map((entry) => {
                const chosen = result?.case === entry.name;
                const answered = chosen ? result?.outline?.rejection : undefined;
                return (
                  <button
                    type="button"
                    key={entry.name}
                    className="entry"
                    data-case={entry.name}
                    aria-selected={chosen}
                    disabled={busy}
                    onClick={() => onCase(entry.name)}
                  >
                    <span className="name">{entry.name.replace(/_/g, " ")}</span>
                    {chosen && result ? (
                      result.applied ? (
                        <span
                          className={`pill ${result.outline?.accepted ? "no" : "ok"}`}
                          data-role="answered"
                        >
                          {result.outline?.accepted ? "ACCEPTED — a defect" : answered}
                        </span>
                      ) : (
                        <span className="pill">not applicable</span>
                      )
                    ) : (
                      <span className="pill">{entry.expect}</span>
                    )}
                    {chosen && result && (
                      <span className="why">
                        {result.applied
                          ? result.outline?.accepted
                            ? "This forgery was accepted. That is a finding, not a demonstration."
                            : answered === entry.expect
                              ? "Refused at the rule this mutation is pinned to."
                              : `Refused at ${answered}, earlier than the rule this mutation is pinned to on the harness's own models. The defect is visible sooner here.`
                          : result.why}
                      </span>
                    )}
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      </div>

      <div style={{ display: "grid", gap: "20px" }}>
        <div className="card">
          <header>
            <h2>Field editor</h2>
            <span className="note">change a byte, reseal, ask again</span>
          </header>
          <div className="body">
            <div style={{ display: "grid", gap: "10px" }}>
              <label>
                record
                <select
                  value={record}
                  data-role="edit-record"
                  onChange={(event) => {
                    setRecord(Number(event.target.value));
                    setOffset(0);
                  }}
                >
                  <option value={-1}>choose one</option>
                  {records.map((step) => (
                    <option key={step.id} value={step.id}>
                      #{step.id} {step.kind} ({step.length} bytes)
                    </option>
                  ))}
                </select>
              </label>
              <label>
                byte within it
                <input
                  type="number"
                  min={0}
                  max={target ? Math.max(0, target.length - 1) : 0}
                  value={offset}
                  data-role="edit-offset"
                  disabled={!target}
                  onChange={(event) => setOffset(Math.max(0, Number(event.target.value) || 0))}
                />
                <span className="pill">
                  {current === null ? "—" : `now 0x${current.toString(16).padStart(2, "0")}`}
                </span>
              </label>
              <label>
                new value
                <input
                  type="number"
                  min={0}
                  max={255}
                  value={value}
                  data-role="edit-value"
                  onChange={(event) =>
                    setValue(Math.min(255, Math.max(0, Number(event.target.value) || 0)))
                  }
                />
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={reseal}
                  data-role="edit-reseal"
                  onChange={(event) => setReseal(event.target.checked)}
                />
                repair the checksum afterwards
              </label>
              <div>
                <button
                  type="button"
                  className="act"
                  data-role="edit-apply"
                  disabled={busy || !target}
                  onClick={() => onEdit(at, value, reseal)}
                >
                  forge it
                </button>
              </div>
            </div>
          </div>
        </div>

        <div className="card">
          <header>
            <h2>What the verifier said</h2>
            {edited && (
              <span className={`note ${edited.outline.accepted ? "num bad" : ""}`}>
                {edited.outline.accepted ? "accepted" : (edited.outline.rejection ?? "refused")}
              </span>
            )}
          </header>
          <div className="body">
            {edited ? (
              <>
                <dl className="facts">
                  <dt>byte</dt>
                  <dd>{edited.at}</dd>
                  <dt>changed</dt>
                  <dd>
                    0x{edited.from.toString(16).padStart(2, "0")} → 0x
                    {edited.to.toString(16).padStart(2, "0")}
                  </dd>
                  <dt>checksum</dt>
                  <dd>{edited.sealed ? "repaired in this page" : "left wrong"}</dd>
                  <dt>answer</dt>
                  <dd
                    className={edited.outline.accepted ? "num bad" : "num exact"}
                    data-role="edit-answer"
                  >
                    {edited.outline.accepted
                      ? "accepted"
                      : (edited.outline.rejection ?? "refused")}
                  </dd>
                </dl>
                {edited.outline.detail && (
                  <p className="prose">{edited.outline.detail}</p>
                )}
                <div className="hex" style={{ marginBottom: 0 }}>
                  {edited.hex.slice(0, edited.at * 2)}
                  <em>{edited.hex.slice(edited.at * 2, edited.at * 2 + 2)}</em>
                  {edited.hex.slice(edited.at * 2 + 2)}
                </div>
              </>
            ) : (
              <p className="prose" style={{ margin: 0 }}>
                Nothing has been edited yet.
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
