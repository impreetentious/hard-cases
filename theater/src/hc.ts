// The page's side of the WebAssembly interface.
//
// The whole of it. Bytes go in through a reserve, an answer pointer comes
// back, the length is read from the four bytes in front of it, and the answer
// is released. There is no binding generator anywhere in this project and this
// file is the reason there does not need to be one.
//
// Every call returns parsed JSON, and every failure comes back as a report
// with a `refused` message rather than as an exception, because the module
// answers rather than traps.

export type Provenance = "live" | "replay";

/** One event of a trace, as the schema writes it. */
export interface TraceEvent {
  kind: string;
  node?: number;
  parent?: number;
  depth?: number;
  iterations?: number;
  /** A float where the search produced one; a string where the value is exact. */
  objective?: number | string;
  bound?: string;
  split?: string;
  variable?: number;
  identifier?: number;
  scope?: number;
  family?: string;
  violation?: number;
  source?: string;
  lower?: number | string;
  upper?: number | string;
  gap?: string;
  reason?: string;
  stage?: string;
  bytes?: number;
  checksum?: number;
  accepted?: boolean;
  rejection?: string | null;
  work?: number;
  status?: string;
  rule?: number;
  position?: number;
}

export interface Trace {
  version: number;
  instance: string;
  events: TraceEvent[];
}

export interface Report {
  provenance: Provenance | null;
  refused?: string;
  instance?: {
    digest: string;
    format: number;
    rows: number;
    columns: number;
    nonzeros: number;
  };
  /**
   * The reduced model's column labels, when the run had a reduction chain.
   *
   * A branch cites a column of the model the search was about, which presolve
   * may have removed columns from, so these are the only labels that can name
   * one correctly. A replay carries no chain and this is null; the panels fall
   * back to the index rather than to a name that might belong to another
   * column.
   */
  reduced?: { columns: string[] } | null;
  certificate?: { bytes: number; hex: string } | null;
  verdict?: { accepted: boolean; rejection: string | null; work: number } | null;
  trace?: Trace;
}

/** One record of a certificate, as the verifier's own reader saw it. */
export interface ProofRecord {
  id: number;
  code: number;
  kind: string;
  /** Where the record's payload sits in the certificate. */
  at: number;
  length: number;
  node?: number;
  parent?: number;
  scope?: number;
  variable?: number;
  split?: string;
  left?: number;
  right?: number;
  cites?: number;
  terms?: number;
  width?: number;
  cover?: number;
  family?: string;
  rule?: number;
  claim?: string;
  /** Exact quantities are strings, everywhere, on purpose. */
  stated?: string | null;
  lower?: string | null;
  gap?: string | null;
  step?: string | null;
  kinds?: number;
}

/** A certificate as the verifier reads it, with the verdict beside it. */
export interface Outline {
  read: boolean;
  accepted: boolean;
  rejection?: string | null;
  detail?: string;
  claim?: string;
  tag?: number;
  bytes?: number;
  work?: number;
  widest_bits?: number;
  objective?: string | null;
  lower?: string | null;
  gap?: string | null;
  steps?: ProofRecord[];
}

/** One iteration of the root relaxation, as it happened. */
export interface PivotStep {
  iteration: number;
  phase: number;
  entering: number;
  direction: number;
  leaving: number | null;
  theta: number | null;
  objective: number | null;
  infeasibility: number | null;
  rung: string;
}

/** The root relaxation under the textbook rule, pivot by pivot. */
export interface Glass {
  refused?: string;
  rule: string;
  /** Which model was solved, in words, since it is not the reduced one. */
  model?: string;
  /** Whether the problem was scaled, which the step lengths are in terms of. */
  scaled?: boolean;
  outcome: string;
  iterations: number;
  kept: number;
  truncated: boolean;
  flips: number;
  degenerate: number;
  refactorizations: number;
  escalations: number;
  columns: { name: string; kind: string; integral: boolean }[];
  pivots: PivotStep[];
}

/** One named mutation of the tamper corpus. */
export interface TamperCase {
  name: string;
  expect: string;
  resealed: boolean;
  reattaches: boolean;
}

/** What happened when a mutation was applied. */
export interface Tampered {
  case?: string;
  expect?: string;
  resealed?: boolean;
  applied: boolean;
  why?: string;
  forged?: string;
  outline?: Outline;
}

export interface Verdict {
  accepted: boolean;
  work?: number;
  bytes?: number;
  widest_bits?: number;
  rejection?: string;
  detail?: string;
}

interface Exports {
  memory: WebAssembly.Memory;
  hc_reserve(length: number): number;
  hc_release(pointer: number, length: number): void;
  hc_release_answer(pointer: number): void;
  hc_solve_live(instance: number, length: number, format: number, nodes: number): number;
  hc_replay(
    trace: number,
    traceLength: number,
    proof: number,
    proofLength: number,
    instance: number,
    instanceLength: number,
    format: number,
  ): number;
  hc_recheck(proof: number, proofLength: number, instance: number, instanceLength: number): number;
  hc_pivots(instance: number, length: number, format: number, limit: number): number;
  hc_outline(proof: number, proofLength: number, instance: number, instanceLength: number): number;
  hc_tamper_cases(): number;
  hc_tamper(
    name: number,
    nameLength: number,
    proof: number,
    proofLength: number,
    instance: number,
    instanceLength: number,
    other: number,
    otherLength: number,
  ): number;
  hc_version(): number;
}

const decoder = new TextDecoder();

/** The solver, loaded once and driven directly. */
export class Solver {
  private constructor(
    private readonly api: Exports,
    /** The workspace version the module was built from. */
    readonly version: string,
    /** The trace schema version it writes. */
    readonly schema: number,
  ) {}

  /** Fetches and instantiates the module. */
  static async load(url: string): Promise<Solver> {
    const source = await fetch(url);
    if (!source.ok) {
      throw new Error(`the solver module did not load: ${source.status}`);
    }
    const { instance } = await WebAssembly.instantiate(await source.arrayBuffer(), {});
    const api = instance.exports as unknown as Exports;
    const stated = JSON.parse(readAnswer(api, api.hc_version())) as {
      version: string;
      trace_schema: number;
    };
    return new Solver(api, stated.version, stated.trace_schema);
  }

  /** Solves an instance here, now, under a node budget. */
  solve(instance: Uint8Array, format: number, nodes: number): Report {
    const held = this.give(instance);
    try {
      return JSON.parse(
        readAnswer(this.api, this.api.hc_solve_live(held.pointer, held.length, format, nodes)),
      ) as Report;
    } finally {
      this.take(held);
    }
  }

  /** Replays a recorded trace and certificate against the instance they name. */
  replay(trace: Uint8Array, proof: Uint8Array, instance: Uint8Array, format: number): Report {
    const one = this.give(trace);
    const two = this.give(proof);
    const three = this.give(instance);
    try {
      return JSON.parse(
        readAnswer(
          this.api,
          this.api.hc_replay(
            one.pointer,
            one.length,
            two.pointer,
            two.length,
            three.pointer,
            three.length,
            format,
          ),
        ),
      ) as Report;
    } finally {
      this.take(one);
      this.take(two);
      this.take(three);
    }
  }

  /** Rechecks a certificate and reports only what the verifier said. */
  recheck(proof: Uint8Array, instance: Uint8Array): Verdict {
    const one = this.give(proof);
    let two: { pointer: number; length: number } | null = null;
    try {
      two = this.give(instance);
      return JSON.parse(
        readAnswer(this.api, this.api.hc_recheck(one.pointer, one.length, two.pointer, two.length)),
      ) as Verdict;
    } finally {
      this.take(one);
      if (two) this.take(two);
    }
  }

  /** Runs the root relaxation under the textbook rule and reports every pivot. */
  glass(instance: Uint8Array, format: number, limit: number): Glass {
    const held = this.give(instance);
    try {
      return JSON.parse(
        readAnswer(this.api, this.api.hc_pivots(held.pointer, held.length, format, limit)),
      ) as Glass;
    } finally {
      this.take(held);
    }
  }

  /** Reads a certificate the way the verifier reads it. */
  outline(proof: Uint8Array, instance: Uint8Array): Outline {
    const one = this.give(proof);
    let two: { pointer: number; length: number } | null = null;
    try {
      two = this.give(instance);
      return JSON.parse(
        readAnswer(this.api, this.api.hc_outline(one.pointer, one.length, two.pointer, two.length)),
      ) as Outline;
    } finally {
      this.take(one);
      if (two) this.take(two);
    }
  }

  /** The tamper corpus this module carries. */
  tamperCases(): TamperCase[] {
    return JSON.parse(readAnswer(this.api, this.api.hc_tamper_cases())) as TamperCase[];
  }

  /** Applies one named mutation and reports what the verifier then said. */
  tamper(name: string, proof: Uint8Array, instance: Uint8Array, other: Uint8Array): Tampered {
    const label = this.give(new TextEncoder().encode(name));
    const one = this.give(proof);
    const two = this.give(instance);
    const three = this.give(other);
    try {
      return JSON.parse(
        readAnswer(
          this.api,
          this.api.hc_tamper(
            label.pointer,
            label.length,
            one.pointer,
            one.length,
            two.pointer,
            two.length,
            three.pointer,
            three.length,
          ),
        ),
      ) as Tampered;
    } finally {
      this.take(label);
      this.take(one);
      this.take(two);
      this.take(three);
    }
  }

  private give(data: Uint8Array): { pointer: number; length: number } {
    const pointer = this.api.hc_reserve(data.length);
    try {
      new Uint8Array(this.api.memory.buffer, pointer, data.length).set(data);
    } catch (error) {
      this.api.hc_release(pointer, data.length);
      throw error;
    }
    return { pointer, length: data.length };
  }

  private take(held: { pointer: number; length: number }): void {
    this.api.hc_release(held.pointer, held.length);
  }
}

/** Reads a length-prefixed answer out of the module and releases it. */
function readAnswer(api: Exports, pointer: number): string {
  try {
    const length = new DataView(api.memory.buffer).getUint32(pointer, true);
    return decoder.decode(new Uint8Array(api.memory.buffer, pointer + 4, length));
  } finally {
    api.hc_release_answer(pointer);
  }
}

/**
 * Hexadecimal text as bytes, for a certificate that came back as a string.
 *
 * Refuses rather than guesses. An odd number of characters or anything outside
 * the alphabet is a document this page did not receive from the module, and
 * `Number.parseInt` would quietly answer `NaN` — which becomes zero in a typed
 * array, so a malformed string would arrive as a certificate full of holes and
 * be blamed on the verifier.
 */
export function fromHex(text: string): Uint8Array {
  if (text.length % 2 !== 0 || /[^0-9a-fA-F]/.test(text)) {
    throw new Error("that is not hexadecimal");
  }
  const out = new Uint8Array(text.length / 2);
  for (let at = 0; at < out.length; at += 1) {
    out[at] = Number.parseInt(text.slice(at * 2, at * 2 + 2), 16);
  }
  return out;
}

/** Bytes as hexadecimal, for a certificate a panel is about to damage. */
export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

// The polynomial the certificate format seals with, as a table built once.
//
// Written here rather than reached for through the module, and that is the
// point rather than an oversight. The lab lets a visitor change a field and
// then repair the seal, which is exactly what a forger with the format
// specification would do; a lab that asked the solver to repair it would be
// demonstrating that the solver cooperates with forgery rather than that the
// verifier survives it.
const CRC32C_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let byte = 0; byte < 256; byte += 1) {
    let value = byte;
    for (let bit = 0; bit < 8; bit += 1) {
      value = value & 1 ? (value >>> 1) ^ 0x82f6_3b78 : value >>> 1;
    }
    table[byte] = value >>> 0;
  }
  return table;
})();

/** The checksum a certificate carries over everything in front of it. */
export function crc32c(bytes: Uint8Array): number {
  let value = 0xffff_ffff;
  for (const byte of bytes) {
    value = (value >>> 8) ^ (CRC32C_TABLE[(value ^ byte) & 0xff] ?? 0);
  }
  return (value ^ 0xffff_ffff) >>> 0;
}

/** Repairs the trailing checksum over whatever the bytes now are. */
export function resealed(bytes: Uint8Array): Uint8Array {
  const out = bytes.slice();
  const split = out.length - 4;
  const sum = crc32c(out.subarray(0, split));
  new DataView(out.buffer, out.byteOffset).setUint32(split, sum, true);
  return out;
}
