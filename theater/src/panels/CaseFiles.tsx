// The case files.
//
// Real defects, minimized, with the harness that found them and the change
// that closed them. There is no target number of these and no incentive to
// reach one: an entry exists because a defect did, and a padded catalogue
// would be worth less than an empty one.
//
// They sit in the situation room rather than only in the repository because
// they are the honest half of the demonstration. The other panels show a
// verifier catching forgeries; these show it catching us.

import { useState } from "react";

/** One case file, as the build step parsed it out of its Markdown. */
export interface CaseFile {
  id: string;
  crate?: string;
  "found-by"?: string;
  reproduce?: string;
  "fixed-in"?: string;
  rejection?: string;
  title: string;
  sections: Record<string, string>;
}

const ORDER = ["symptom", "hunt", "root cause", "fix", "lesson"];

/** Paragraphs, with the inline code the frontmatter style uses left alone. */
function paragraphs(text: string) {
  return text.split(/\n{2,}/).map((block, at) => (
    <p key={at} style={{ marginTop: at === 0 ? 0 : "0.9em" }}>
      {block.split(/(`[^`]+`)/).map((piece, index) =>
        piece.startsWith("`") && piece.endsWith("`") ? (
          <code key={index} className="num">
            {piece.slice(1, -1)}
          </code>
        ) : (
          <span key={index}>{piece}</span>
        ),
      )}
    </p>
  ));
}

export default function CaseFiles({ cases }: { cases: CaseFile[] }) {
  // The catalogue arrives after the first render, so an initial value taken
  // from it is taken from an empty list and the panel opens on nothing. The
  // choice is resolved at render instead: whatever was picked, or the first
  // entry once there is one.
  const [open, setOpen] = useState<string | null>(null);
  const chosen = cases.find((entry) => entry.id === open) ?? cases[0] ?? null;

  if (cases.length === 0) {
    return <div className="empty">The catalogue did not load.</div>;
  }

  return (
    <div className="panel split">
      <div className="card">
        <header>
          <h2>{chosen?.id ?? "Case file"}</h2>
          <span className="note">{chosen?.["fixed-in"]}</span>
        </header>
        <div className="body">
          {chosen ? (
            <>
              <h3 style={{ marginTop: 0, fontSize: "16px" }}>{chosen.title}</h3>
              <dl className="facts" style={{ marginBottom: "24px" }}>
                <dt>crate</dt>
                <dd>{chosen.crate ?? "—"}</dd>
                <dt>found by</dt>
                <dd style={{ fontFamily: "inherit" }}>{chosen["found-by"] ?? "—"}</dd>
                <dt>reproduce</dt>
                <dd>{chosen.reproduce ?? "—"}</dd>
                {chosen.rejection && (
                  <>
                    <dt>turns on</dt>
                    <dd className="num exact">{chosen.rejection}</dd>
                  </>
                )}
              </dl>
              {ORDER.map((heading) =>
                chosen.sections[heading] ? (
                  <section key={heading} style={{ marginBottom: "22px" }}>
                    <h4
                      style={{
                        margin: "0 0 8px",
                        fontSize: "11px",
                        letterSpacing: "0.1em",
                        textTransform: "uppercase",
                        color: "var(--ink-faint)",
                      }}
                    >
                      {heading}
                    </h4>
                    <div className="prose" style={{ maxWidth: "72ch" }}>
                      {paragraphs(chosen.sections[heading])}
                    </div>
                  </section>
                ) : (
                  <p key={heading} className="prose">
                    This case file has no <strong>{heading}</strong> section, and the
                    catalogue requires one.
                  </p>
                ),
              )}
            </>
          ) : (
            <p className="prose">Choose a case.</p>
          )}
        </div>
      </div>

      <div className="card">
        <header>
          <h2>Catalogue</h2>
          <span className="note">{cases.length}</span>
        </header>
        <div className="body">
          <div className="list">
            {cases.map((entry) => (
              <button
                type="button"
                key={entry.id}
                className="entry"
                data-case-file={entry.id}
                aria-selected={entry.id === chosen?.id}
                onClick={() => setOpen(entry.id)}
              >
                <span className="name">{entry.id}</span>
                <span className="pill">{entry["fixed-in"]}</span>
                <span className="why">{entry.title}</span>
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
