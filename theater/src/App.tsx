// The situation room.
//
// Everything on this page comes from one of exactly two places, and which one
// is stated at all times in the header: a solve this module performed here and
// now, or a trace and a certificate a native run recorded and this module
// replayed. There is no third source. A panel with nothing behind it says so
// and stays empty, because a room that filled its empty panels with plausible
// shapes would be the opposite of what this whole project is for.
//
// The second rule, stated in the same header, is the one about numbers. A
// quantity out of the floating-point search is amber, dashed, and prefixed
// with a tilde; a quantity the independent verifier recomputed is cyan, solid,
// and exact. They are different types in the schema and different colours here
// and nothing on the page converts one into the other.

import { useCallback, useEffect, useMemo, useState } from "react";

import type { Glass as GlassReport, Outline, Report, TamperCase, Tampered } from "./hc";
import { Solver, fromHex, resealed, toHex } from "./hc";
import { viewAt } from "./state";
import type { Cut } from "./state";
import * as share from "./share";
import { CEILING } from "./share";
import { digest } from "./format";
import Tree from "./panels/Tree";
import Convergence from "./panels/Convergence";
import Frontier from "./panels/Frontier";
import Glass from "./panels/Glass";
import Certificate from "./panels/Certificate";
import Tamper from "./panels/Tamper";
import CaseFiles from "./panels/CaseFiles";
import type { CaseFile } from "./panels/CaseFiles";

/**
 * A file the page ships beside itself.
 *
 * Resolved against the document rather than against a build-time constant,
 * because this page is served from wherever it is put — the root in
 * development, a project path when it is published — and a base baked in at
 * build time is a base that is right in exactly one of those.
 */
function asset(path: string): string {
  return new URL(path, document.baseURI).href;
}

/**
 * The format an opened file starts on, which is a starting position and not a
 * reading of the file.
 *
 * The extension is the only thing there is to go on and it is not evidence:
 * the same bytes read under two formats are two instances here, because the
 * format is part of the instance digest. So this picks somewhere for the
 * control to start and the control is always visible, always changeable, and
 * never hidden once a file is open.
 */
function guessNothing(name: string): number {
  if (name.endsWith(".lp")) return 3;
  if (name.endsWith(".json")) return 4;
  return 1;
}

interface Offer {
  name: string;
  format: number;
  covers: string;
  bytes: number;
}

/** A file the visitor opened, which is not in the catalogue and never will be. */
interface Own {
  name: string;
  bytes: Uint8Array;
  format: number;
}

/**
 * The formats a reader may name, with the tag each one travels as.
 *
 * A select rather than a guess, and that is the project's rule rather than a
 * UI preference: the format is part of the instance digest, so the same bytes
 * read two ways are two instances, and a heuristic here would make the
 * identity of a model depend on one.
 */
const FORMATS: { tag: number; name: string }[] = [
  { tag: 1, name: "MPS, fixed columns" },
  { tag: 2, name: "MPS, free form" },
  { tag: 3, name: "LP format" },
  { tag: 4, name: "the JSON schema" },
];

interface Built {
  version: string;
}

interface Recording {
  instance: string;
  format: number;
  trace: string;
  certificate: string;
  version: string;
}

const PANELS: { id: string; title: string }[] = [
  { id: "tree", title: "Tree" },
  { id: "convergence", title: "Convergence" },
  { id: "frontier", title: "Frontier" },
  { id: "glass", title: "Glass" },
  { id: "certificate", title: "Certificate" },
  { id: "tamper", title: "Tamper lab" },
  { id: "cases", title: "Case files" },
];

/** Fetches a file the page ships beside itself. */
async function fetchBytes(path: string): Promise<Uint8Array> {
  const answer = await fetch(asset(path));
  if (!answer.ok) throw new Error(`${path} did not load: ${answer.status}`);
  return new Uint8Array(await answer.arrayBuffer());
}

export default function App() {
  const [where, setWhere] = useState<share.Where>(() => share.read(location.hash));
  const [solver, setSolver] = useState<Solver | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [offers, setOffers] = useState<Offer[]>([]);
  const [cases, setCases] = useState<CaseFile[]>([]);
  const [recording, setRecording] = useState<Recording | null>(null);
  const [built, setBuilt] = useState<Built | null>(null);
  const [corpus, setCorpus] = useState<TamperCase[]>([]);

  const [own, setOwn] = useState<Own | null>(null);
  const [instance, setInstance] = useState<Uint8Array | null>(null);
  const [report, setReport] = useState<Report | null>(null);
  const [outline, setOutline] = useState<Outline | null>(null);
  const [glass, setGlass] = useState<GlassReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [chosenNode, setChosenNode] = useState<number | null>(null);
  const [cut, setCut] = useState<Cut | null>(null);
  const [forged, setForged] = useState<Tampered | null>(null);
  const [edited, setEdited] = useState<{
    outline: Outline;
    at: number;
    from: number;
    to: number;
    sealed: boolean;
    hex: string;
  } | null>(null);
  const [playing, setPlaying] = useState(false);

  const move = useCallback((change: Partial<share.Where>) => {
    setWhere((was) => ({ ...was, ...change }));
  }, []);

  // The address is the state. Written on every change and read once, so a
  // pasted link opens on exactly what the sender was looking at.
  useEffect(() => {
    const text = share.write(where);
    if (text !== location.hash && !(text === "" && location.hash === "")) {
      history.replaceState(null, "", text === "" ? location.pathname : text);
    }
  }, [where]);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [loaded, catalogue, files, recorded, stamped] = await Promise.all([
          Solver.load(asset("hc.wasm")),
          fetch(asset("instances/catalogue.json")).then((answer) => answer.json()),
          fetch(asset("case-files.json")).then((answer) => answer.json()),
          fetch(asset("recorded/run.json")).then((answer) => answer.json()),
          fetch(asset("built.json")).then((answer) => answer.json()),
        ]);
        if (!alive) return;
        setSolver(loaded);
        setBuilt(stamped as Built);
        setOffers(catalogue as Offer[]);
        setCases(files as CaseFile[]);
        setRecording(recorded as Recording);
        setCorpus(loaded.tamperCases());
      } catch (error) {
        if (alive) setFailure(error instanceof Error ? error.message : String(error));
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  const offer = useMemo(
    () => offers.find((entry) => entry.name === where.instance) ?? offers[0] ?? null,
    [offers, where.instance],
  );
  /** What is actually being solved: an opened file, or whatever the picker is on. */
  const chosenName = own?.name ?? offer?.name ?? "";
  const chosenFormat = own?.format ?? offer?.format ?? 1;

  // The bytes of whatever the picker is on, fetched once per instance. An
  // opened file is already in hand and is not fetched.
  useEffect(() => {
    let alive = true;
    if (own) {
      setInstance(own.bytes);
      return;
    }
    if (!offer) return;
    setInstance(null);
    fetchBytes(`instances/${offer.name}`)
      .then((bytes) => {
        if (alive) setInstance(bytes);
      })
      .catch((error) => {
        if (alive) setFailure(String(error));
      });
    return () => {
      alive = false;
    };
  }, [offer, own]);

  const clear = useCallback(() => {
    setReport(null);
    setOutline(null);
    setGlass(null);
    setForged(null);
    setEdited(null);
    setChosenNode(null);
    setCut(null);
    setPlaying(false);
  }, []);

  const solve = useCallback(() => {
    if (!solver || !instance) return;
    setBusy(true);
    clear();
    // A frame, so the button paints its disabled state before a solve that may
    // take a moment blocks the thread. The alternative is a page that looks
    // frozen exactly when it is doing the thing it exists to do.
    requestAnimationFrame(() => {
      try {
        const answer = solver.solve(instance, chosenFormat, where.nodes);
        setReport(answer);
        if (answer.certificate) {
          setOutline(solver.outline(fromHex(answer.certificate.hex), instance));
        }
        setGlass(solver.glass(instance, chosenFormat, 512));
        move({ event: -1 });
      } catch (error) {
        setFailure(error instanceof Error ? error.message : String(error));
      } finally {
        // In a `finally` rather than at the end: a solve that threw would
        // otherwise leave the button disabled and the page looking busy
        // forever, which is a worse failure than the one that caused it.
        setBusy(false);
      }
    });
  }, [solver, instance, chosenFormat, where.nodes, clear, move]);

  const replay = useCallback(() => {
    if (!solver || !recording) return;
    setBusy(true);
    clear();
    setOwn(null);
    (async () => {
      try {
        const [trace, proof, model] = await Promise.all([
          fetchBytes(`recorded/${recording.trace}`),
          fetchBytes(`recorded/${recording.certificate}`),
          fetchBytes(`instances/${recording.instance}`),
        ]);
        const answer = solver.replay(trace, proof, model, recording.format);
        setReport(answer);
        setOutline(solver.outline(proof, model));
        setGlass(solver.glass(model, recording.format, 512));
        move({ instance: recording.instance, event: -1 });
      } catch (error) {
        setFailure(String(error));
      } finally {
        setBusy(false);
      }
    })();
  }, [solver, recording, clear, move]);

  const trace = report?.trace;
  const events = trace?.events.length ?? 0;
  const cursor = where.event < 0 || where.event >= events ? events - 1 : where.event;
  const view = useMemo(() => viewAt(trace, cursor), [trace, cursor]);

  // Playing walks the cursor forward one event at a time and stops at the end,
  // which is the whole of the animation on this page. The tree is not being
  // drawn as it is discovered; it is being redrawn from the events already
  // recorded, and the scrubber says so by existing.
  useEffect(() => {
    if (!playing || events === 0) return undefined;
    const timer = window.setInterval(() => {
      setWhere((was) => {
        const at = was.event < 0 ? events - 1 : was.event;
        if (at >= events - 1) return { ...was, event: events - 1 };
        return { ...was, event: at + 1 };
      });
    }, 110);
    return () => window.clearInterval(timer);
  }, [playing, events]);
  useEffect(() => {
    if (cursor >= events - 1) setPlaying(false);
  }, [cursor, events]);

  const proofHex = report?.certificate?.hex ?? null;
  // The labels a split can be named by, and an empty list when the run carried
  // no reduction chain to name them from.
  const columns = report?.reduced?.columns ?? [];

  const onCase = useCallback(
    (name: string) => {
      if (!solver || !instance || !proofHex) return;
      const other = offers.find((entry) => entry.name !== chosenName);
      setBusy(true);
      (async () => {
        try {
          const second = other ? await fetchBytes(`instances/${other.name}`) : new Uint8Array();
          setForged(solver.tamper(name, fromHex(proofHex), instance, second));
        } catch (error) {
          setFailure(error instanceof Error ? error.message : String(error));
        } finally {
          setBusy(false);
        }
      })();
    },
    [solver, instance, proofHex, chosenName, offers],
  );

  const onEdit = useCallback(
    (at: number, value: number, reseal: boolean) => {
      if (!solver || !instance || !proofHex) return;
      let bytes: Uint8Array;
      try {
        bytes = fromHex(proofHex);
      } catch (error) {
        setFailure(error instanceof Error ? error.message : String(error));
        return;
      }
      if (at < 0 || at >= bytes.length) return;
      const from = bytes[at] ?? 0;
      bytes[at] = value;
      const sent = reseal ? resealed(bytes) : bytes;
      setEdited({
        outline: solver.outline(sent, instance),
        at,
        from,
        to: value,
        sealed: reseal,
        // The forged bytes, so a reader can see what they made rather than
        // being told about it — and copy them out and check them elsewhere.
        hex: toHex(sent),
      });
    },
    [solver, instance, proofHex],
  );

  const provenance = report?.refused ? "refused" : (report?.provenance ?? null);
  // The version the page was bundled beside against the version the module it
  // loaded reports. They are collected by the same build step, so a difference
  // means the page is reporting on a solver nobody is running — which is the
  // failure that step exists to prevent, and it should be visible rather than
  // assumed away.
  const stale = Boolean(built && solver && built.version !== solver.version);

  return (
    <div className="room">
      <header className="masthead">
        <h1>
          Hard Cases <span>· the situation room</span>
        </h1>
        <span
          className={`stamp ${provenance ?? ""}`}
          data-role="provenance"
          title="where everything on this page came from"
        >
          {provenance === "live"
            ? "live · solved here"
            : provenance === "replay"
              ? "replay · recorded natively"
              : provenance === "refused"
                ? "refused"
                : "nothing solved yet"}
        </span>
        <div className="legend">
          <span>
            <i className="float-key" />
            float search
          </span>
          <span>
            <i className="exactish-key" />
            exact, unchecked
          </span>
          <span>
            <i className="exact-key" />
            verified
          </span>
        </div>
        <span className="spacer" />
        <span
          className={`stamp ${stale ? "refused" : ""}`}
          data-role="version"
          title={
            stale
              ? "the page was bundled beside one version of the solver and loaded another"
              : "the version the loaded module reports"
          }
        >
          solver {solver?.version ?? "…"} · trace schema {solver?.schema ?? "…"}
          {stale ? ` · bundled beside ${built?.version}` : ""}
        </span>
      </header>

      <div className="controls">
        <label>
          instance
          <select
            value={own ? "" : (offer?.name ?? "")}
            data-role="instance"
            onChange={(event) => {
              clear();
              setOwn(null);
              move({ instance: event.target.value, event: -1, record: -1 });
            }}
          >
            {own && <option value="">{own.name} — your file</option>}
            {offers.map((entry) => (
              <option key={entry.name} value={entry.name}>
                {entry.name} — {entry.covers}
              </option>
            ))}
          </select>
        </label>
        <label>
          your own
          <input
            type="file"
            data-role="open"
            style={{ maxWidth: "13rem" }}
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (!file) return;
              clear();
              file
                .arrayBuffer()
                .then((buffer) => {
                  setOwn({
                    name: file.name,
                    bytes: new Uint8Array(buffer),
                    format: guessNothing(file.name),
                  });
                  move({ event: -1, record: -1 });
                })
                .catch((error) => setFailure(String(error)));
            }}
          />
        </label>
        {own && (
          <label>
            read as
            <select
              value={own.format}
              data-role="format"
              onChange={(event) => {
                clear();
                setOwn({ ...own, format: Number(event.target.value) });
              }}
            >
              {FORMATS.map((format) => (
                <option key={format.tag} value={format.tag}>
                  {format.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          node budget
          <input
            type="number"
            min={1}
            max={CEILING}
            title={`at most ${CEILING}: the solve runs on this tab's own thread`}
            value={where.nodes}
            data-role="nodes"
            onChange={(event) =>
              move({
                nodes: Math.max(1, Math.min(CEILING, Number(event.target.value) || 1)),
              })
            }
          />
        </label>
        <button
          type="button"
          className="act"
          data-role="solve"
          disabled={!solver || !instance || busy}
          onClick={solve}
        >
          {busy ? "working…" : "solve here"}
        </button>
        <button
          type="button"
          className="act quiet"
          data-role="replay"
          disabled={!solver || !recording || busy}
          onClick={replay}
        >
          replay a recorded run
        </button>
        {report?.instance && (
          <span className="pill" data-role="digest">
            {digest(report.instance.digest)} · {report.instance.rows}×
            {report.instance.columns}
          </span>
        )}
        {report?.verdict && (
          <span
            className={`pill ${report.verdict.accepted ? "ok" : "no"}`}
            data-role="verdict"
            role="status"
            aria-live="polite"
          >
            {report.verdict.accepted
              ? "verifier accepted"
              : (report.verdict.rejection ?? "verifier refused")}
          </span>
        )}
        <nav className="tabs" role="tablist" aria-label="panels">
          {PANELS.map((panel) => (
            <button
              type="button"
              key={panel.id}
              role="tab"
              id={`tab-${panel.id}`}
              aria-controls="stage"
              data-panel={panel.id}
              aria-selected={where.panel === panel.id}
              onClick={() => move({ panel: panel.id })}
            >
              {panel.title}
            </button>
          ))}
        </nav>
      </div>

      <main
        className="stage"
        id="stage"
        role="tabpanel"
        aria-labelledby={`tab-${where.panel}`}
        tabIndex={-1}
      >
        {failure ? (
          <div className="empty">{failure}</div>
        ) : report?.refused && where.panel !== "cases" ? (
          // The case files are about this repository rather than about the
          // run, so a file that would not read blanks the panels that depend
          // on a run and leaves the one that does not.
          <div className="empty">{report.refused}</div>
        ) : where.panel === "tree" ? (
          <Tree
            view={view}
            chosen={chosenNode}
            onChoose={setChosenNode}
            cut={cut}
            onCut={setCut}
            columns={columns}
            provenance={report?.provenance ?? null}
          />
        ) : where.panel === "convergence" ? (
          <Convergence view={view} trace={trace} cursor={cursor} outline={outline} />
        ) : where.panel === "frontier" ? (
          <Frontier
            view={view}
            outline={outline}
            nodes={where.nodes}
            onNodes={(nodes) => move({ nodes })}
            onCertify={solve}
            busy={busy}
          />
        ) : where.panel === "glass" ? (
          <Glass glass={glass} provenance={report?.provenance ?? null} />
        ) : where.panel === "certificate" ? (
          <Certificate
            outline={outline}
            hex={proofHex}
            chosen={where.record}
            onChoose={(record) => move({ record })}
            columns={columns}
          />
        ) : where.panel === "tamper" ? (
          <Tamper
            cases={corpus}
            outline={outline}
            hex={proofHex}
            result={forged}
            edited={edited}
            busy={busy}
            onCase={onCase}
            onEdit={onEdit}
          />
        ) : (
          <CaseFiles cases={cases} />
        )}
      </main>

      <footer className="scrub">
        <button
          type="button"
          className="act quiet"
          data-role="play"
          disabled={events === 0}
          onClick={() => {
            if (!playing && cursor >= events - 1) move({ event: 0 });
            setPlaying(!playing);
          }}
        >
          {playing ? "pause" : "play"}
        </button>
        <input
          type="range"
          aria-label="the event of the run the panels are showing"
          aria-valuetext={
            events === 0
              ? "no trace"
              : `event ${cursor + 1} of ${events}, ${trace?.events[cursor]?.kind ?? ""}`
          }
          min={0}
          max={Math.max(0, events - 1)}
          value={Math.max(0, cursor)}
          data-role="scrub"
          disabled={events === 0}
          onChange={(event) => move({ event: Number(event.target.value) })}
        />
        <span className="where" data-role="cursor">
          {events === 0
            ? "no trace"
            : `event ${cursor + 1} of ${events} · ${trace?.events[cursor]?.kind ?? ""}`}
        </span>
        <button
          type="button"
          className="act quiet"
          data-role="copy-link"
          disabled={own !== null}
          title={
            own
              ? "a link cannot carry your file; it stays in this tab and reaches no server"
              : "the address carries the instance, the budget, the panel, the event and the record"
          }
          onClick={() => navigator.clipboard?.writeText(location.href)}
        >
          {own ? "not shareable" : "copy link"}
        </button>
      </footer>
    </div>
  );
}
