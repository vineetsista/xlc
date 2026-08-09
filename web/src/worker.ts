// Engine worker: hosts a wasm Session so no workbook — however large —
// can block the page's main thread. main.ts instantiates this file
// twice: an interactive instance (prepare / what-if / sweep, serving the
// latest-wins slider pipeline) and an analysis instance (tornado /
// monte-carlo / diff) with independent Session state, so a long tornado
// or 10k-scenario run never contends with the live slider.
import init, { Session, diff_books } from '../pkg/xlc_wasm.js';

let session: Session | null = null;
const ready = init();

// The ±10% sweep runs entirely in-worker: one message out, one result
// back, and the repeated prepare() calls (the expensive part on large
// workbooks) never touch the page.
function tornado(args: any): string {
  const s = session!;
  const rows: { name: string; loOut: number; hiOut: number }[] = [];
  let base: number | null = null;
  const at = (x: number): number | null => {
    const o = JSON.parse(s.what_if(x));
    return o.ok && o.output !== undefined ? (o.output as number) : null;
  };
  for (const c of args.cands) {
    const p = JSON.parse(s.prepare(c.sheet, c.row, c.col));
    if (!p.ok) continue;
    s.set_watch(args.watch.sheet, args.watch.row, args.watch.col);
    const b = at(c.value);
    const lo = at(c.value * 0.9);
    const hi = at(c.value * 1.1);
    if (b === null || lo === null || hi === null) continue; // watch outside this cone
    base ??= b;
    if (Math.abs(lo - b) < 1e-12 && Math.abs(hi - b) < 1e-12) continue; // no effect
    rows.push({ name: c.name, loOut: lo, hiOut: hi });
  }
  return JSON.stringify({ ok: true, base: base ?? 0, rows });
}

self.onmessage = async (e: MessageEvent) => {
  const { id, op, args } = e.data;
  try {
    await ready;
    if (!session && op !== 'open' && op !== 'diff') throw new Error('engine not loaded — drop the workbook again');
    const t0 = performance.now();
    let r: string;
    switch (op) {
      case 'open': {
        try {
          session?.free();
        } catch {}
        session = null;
        session = new Session(new Uint8Array(args.bytes));
        r = args.analyze ? session.analyze() : '{"ok":true}';
        break;
      }
      case 'candidates':
        r = session!.input_candidates(args.n);
        break;
      case 'prepare':
        r = session!.prepare(args.sheet, args.row, args.col);
        break;
      case 'set_watch':
        session!.set_watch(args.sheet, args.row, args.col);
        r = '{"ok":true}';
        break;
      case 'what_if':
        r = session!.what_if(args.x);
        break;
      case 'sweep':
        r = session!.sweep(args.lo, args.hi, args.n);
        break;
      case 'tornado':
        r = tornado(args);
        break;
      case 'monte': {
        const p = JSON.parse(session!.prepare(args.input.sheet, args.input.row, args.input.col));
        if (!p.ok) {
          r = JSON.stringify({ ok: false, error: p.error ?? 'prepare failed' });
          break;
        }
        session!.set_watch(args.watch.sheet, args.watch.row, args.watch.col);
        r = session!.monte(args.kind, args.p1, args.p2, args.p3, args.n);
        break;
      }
      case 'diff':
        r = diff_books(new Uint8Array(args.a), new Uint8Array(args.b), args.n);
        break;
      default:
        throw new Error(`unknown op ${op}`);
    }
    // ms is engine time only — no postMessage round-trip, no queue wait —
    // so the UI's µs readout stays an honest engine number.
    (self as unknown as Worker).postMessage({ id, ok: true, r, ms: performance.now() - t0 });
  } catch (err) {
    // A THROWN error from a Session op (as opposed to an {ok:false}
    // result) is a wasm panic: the instance is poisoned. Free it so
    // later calls fail loudly instead of returning garbage.
    if (op !== 'open' && op !== 'diff') {
      try {
        session?.free();
      } catch {}
      session = null;
    }
    (self as unknown as Worker).postMessage({ id, ok: false, err: String(err) });
  }
};
