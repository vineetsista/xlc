// xlc web surface v5: the v4 staged anatomy with the engine moved off
// the main thread. Two workers host independent wasm Sessions — A
// (interactive: prepare / what-if / sweep, behind a latest-wins slider
// pipeline) and B (analysis: tornado / monte-carlo / diff) — so a heavy
// workbook can never freeze the page: the slider stays live even while
// a tornado or 10,000-scenario run is in flight. Canvases keep their
// backing store between paints (resizing a canvas reallocates it, and
// doing that once per input event is how drags die).
// Everything local (Law 1).
// Charts follow the dataviz method: the mark pair --data/--data2 is
// validated against the surface (CVD dE 29.4 protan, 34.1 normal),
// hover layer always, recessive grid, text in text tokens, legend
// present for the two-series tornado.

import { SAMPLE_B64, SAMPLE_NAME } from './sample';

type Finding = { detector: string; sheet: string; cell: string; formula: string; proof: string };

const $ = (id: string) => document.getElementById(id)!;
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const DATA = '#2f81f7';
const DATA2 = '#db6a2a';
const DIM = '#7d8590';
const RULE = '#21262d';

// ---------- engine workers ----------
type RpcResult = { r: string; ms: number };
type Rpc = (op: string, args?: any) => Promise<RpcResult>;
function makeWorker(): Rpc {
  const w = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
  let seq = 0;
  const pend = new Map<number, [(v: RpcResult) => void, (e: Error) => void]>();
  // A worker that dies (chunk 404 on a redeploy, browser kill) must
  // reject every pending call, or run() hangs at "compiling…" forever
  // and a latched whatifInflight silently kills the slider.
  const failAll = (why: string) => {
    for (const [, p] of pend) p[1](new Error(why));
    pend.clear();
  };
  w.onerror = (e) => failAll(`engine worker failed: ${(e as ErrorEvent).message || 'script error'}`);
  w.onmessageerror = () => failAll('engine worker message error');
  w.onmessage = (e: MessageEvent) => {
    const { id, ok, r, ms, err } = e.data;
    const p = pend.get(id);
    if (!p) return;
    pend.delete(id);
    ok ? p[0]({ r, ms }) : p[1](new Error(err));
  };
  return (op, args = {}) =>
    new Promise((res, rej) => {
      const id = ++seq;
      pend.set(id, [res, rej]);
      w.postMessage({ id, op, args });
    });
}
const A = makeWorker(); // interactive: open+analyze, candidates, prepare, what-if, sweep
const B = makeWorker(); // analysis: tornado, monte-carlo, diff — own Session, never blocks A

let hasSession = false;
// Two generations guard async continuations. wbGen is workbook identity:
// claimed synchronously at intake (before any file-read/hash await) so
// the LAST drop always wins, regardless of which file hashes faster.
// gen is model state: bumped on load, input switch, and watch switch;
// continuations capture it and drop stale results instead of painting.
let wbGen = 0;
let gen = 0;

let bytesA: ArrayBuffer | null = null;
let wbHash = '';
let lastName = '';
let lastAnalysis: any = null;
let findings: Finding[] = [];
let suppressed = new Set<string>();
let activeIdx = -1;
let filter = 'all';
let inputCands: { sheet: number; row: number; col: number; name: string; value: number; impact: number }[] = [];
let curInput: (typeof inputCands)[0] | null = null;
let sweepPts: [number, number][] = [];
let sweepGen = -1; // the gen sweepPts was computed under — goal seek refuses a stale curve
let sliderLo = 0;
let sliderHi = 1;

// ---------- helpers ----------
const fmt = (n: number) => n.toLocaleString('en-US');
const fmtV = (n: number) =>
  Math.abs(n) >= 1000 ? n.toLocaleString('en-US', { maximumFractionDigits: 2 }) : n.toPrecision(6).replace(/\.?0+$/, '');
const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const fkey = (f: Finding) => `${f.detector}|${f.sheet}|${f.cell}`;

async function sha256hex(bytes: ArrayBuffer): Promise<string> {
  const d = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function countUp(el: Element, target: number, suffix: string, prefix: string) {
  if (reduced || target < 50) {
    el.textContent = `${prefix}${fmt(target)}${suffix}`;
    return;
  }
  const t0 = performance.now();
  const dur = 350;
  const tick = (t: number) => {
    const p = Math.min(1, (t - t0) / dur);
    el.textContent = `${prefix}${fmt(Math.round(target * (1 - Math.pow(1 - p, 3))))}${suffix}`;
    if (p < 1) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

let toastTimer = 0;
function toast(msg: string) {
  const t = $('toast');
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (t.hidden = true), 1600);
}

function loadSuppressions(): Set<string> {
  try {
    const raw = localStorage.getItem(`xlc-suppress-${wbHash}`);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}
const saveSuppressions = () => {
  try {
    localStorage.setItem(`xlc-suppress-${wbHash}`, JSON.stringify([...suppressed]));
  } catch {} // storage-blocked contexts: suppression still works for the session
};

// ---------- file intake ----------
const drop = $('drop');
const fileInput = $('file') as HTMLInputElement;
drop.addEventListener('click', (e) => {
  if ((e.target as HTMLElement).id !== 'try-sample') fileInput.click();
});
drop.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' || e.key === ' ') fileInput.click();
});
drop.addEventListener('dragover', (e) => {
  e.preventDefault();
  drop.classList.add('over');
});
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
// Claim the workbook ticket BEFORE reading the file: two rapid drops
// must resolve in drop order, not in file-read-completion order.
function intakeFile(f: File) {
  const ticket = ++wbGen;
  f.arrayBuffer().then((b) => run(b, f.name, ticket));
}
drop.addEventListener('drop', (e) => {
  e.preventDefault();
  drop.classList.remove('over');
  const f = e.dataTransfer?.files?.[0];
  if (f) intakeFile(f);
});
fileInput.addEventListener('change', () => {
  const f = fileInput.files?.[0];
  if (f) intakeFile(f);
});
$('try-sample').addEventListener('click', (e) => {
  e.stopPropagation();
  const bin = atob(SAMPLE_B64);
  const buf = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
  run(buf.buffer, SAMPLE_NAME);
});

// ---------- the three acts ----------
async function run(bytes: ArrayBuffer, name: string, ticket?: number) {
  const wg = ticket ?? ++wbGen;
  if (wg !== wbGen) return; // a later drop superseded this file while it was being read
  ++gen; // every continuation from the previous model is now stale
  hasSession = false;
  lastAnalysis = null;
  bytesA = bytes;
  lastName = name;
  wbHash = await sha256hex(bytes);
  if (wg !== wbGen) return; // superseded while hashing
  suppressed = loadSuppressions();
  // Per-workbook triage state must never survive into the next file: a
  // stale detector filter silently empties the findings list, and a stale
  // findings array lets keyboard triage resurrect the previous workbook
  // after a failed load.
  findings = [];
  filter = 'all';
  activeIdx = -1;
  ($('pipeline') as HTMLElement).hidden = true;
  $('log').hidden = false;
  ($('act1') as HTMLElement).textContent = `compiling ${name}…`;
  $('act2').textContent = '';
  $('act3').textContent = '';
  $('receipt-detail').hidden = true;
  ($('finding-bar') as HTMLElement).hidden = true;
  $('findings').textContent = '';
  $('capability').textContent = '';
  ($('lab') as HTMLElement).hidden = true;
  ($('compare') as HTMLElement).hidden = true;
  ($('monte-out') as HTMLElement).hidden = true;
  ($('tornado-wrap') as HTMLElement).hidden = true;
  ($('tool-tornado') as HTMLElement).hidden = true;
  ($('tornado-status') as HTMLElement).hidden = true;
  ($('next-hint') as HTMLElement).hidden = true;
  ($('goal-read') as HTMLElement).textContent = '';
  ($('goal-target') as HTMLInputElement).value = '';
  ($('whatif-read') as HTMLElement).textContent = '';
  sweepPts = [];
  sweepGen = -1;
  for (const t of ['curve-tip', 'tornado-tip', 'hist-tip']) ($(t) as HTMLElement).hidden = true;
  ($('diff-out') as HTMLElement).hidden = true;
  ($('diff-status') as HTMLElement).textContent = '';
  // Reset per-workbook lab state so a later export can never carry the
  // previous workbook's Monte-Carlo stats or sensitivity table.
  histData = null;
  histMode = 'hist';
  $('view-hist').classList.add('active');
  $('view-cdf').classList.remove('active');
  $('monte-stats').textContent = '';
  tornadoRows = [];
  tornadoBase = 0;
  clearTimeout(tornadoTimer);

  const t0 = performance.now();
  // B opens the same bytes (parse only, no analyze) so tornado/monte/diff
  // get their own engine — but off the audit's critical path: a slow or
  // failing B must neither delay nor kill the audit. B-dependent features
  // surface their own errors through their own catch paths.
  B('open', { bytes, analyze: false }).catch(() => {});
  let a: any;
  try {
    a = JSON.parse((await A('open', { bytes, analyze: true })).r);
  } catch (e) {
    if (wg === wbGen) $('act1').innerHTML = `<span class="err">error:</span> ${esc(String(e))}`;
    return;
  }
  if (wg !== wbGen) return; // superseded by a newer drop
  const secs = (performance.now() - t0) / 1000;
  if (!a.ok) {
    $('act1').innerHTML = `<span class="err">error:</span> ${esc(a.error ?? 'unknown')}`;
    return;
  }
  hasSession = true;
  lastAnalysis = a;
  ($('topnav') as HTMLElement).hidden = false;

  // Act 1 (count-up).
  const act1 = $('act1');
  act1.innerHTML = `compiled <strong><span id="cnt"></span></strong> formulas across ${a.sheets.length} sheet${a.sheets.length === 1 ? '' : 's'} in <strong>${secs.toFixed(2)}s</strong>`;
  countUp(act1.querySelector('#cnt')!, a.formula_cells, '', '');

  // Act 2 (receipt, expandable).
  const r = a.receipt;
  const pct = (r.rate * 100).toFixed(2);
  const cls = r.rate >= 0.97 ? 'ok' : r.rate >= 0.8 ? 'warn' : 'err';
  const excl = Object.values(r.excluded as Record<string, number>).reduce((x, y) => x + y, 0);
  setTimeout(() => {
    if (wg !== wbGen) return; // never paint a superseded file's receipt
    $('act2').innerHTML =
      `receipt: <span class="${cls} landed">${fmt(r.pass)}/${fmt(r.verifiable)} verifiable cells re-derived bit-exact (${pct}%)</span>` +
      (excl > 0 ? ` <span class="dim">· ${fmt(excl)} excluded ▸</span>` : ' <span class="dim">▸</span>');
    const lines = [
      `exact ${fmt(r.pass - r.ulp1 - r.sig15)} · 1-ulp ${fmt(r.ulp1)} · 15-sig-digit ${fmt(r.sig15)}`,
      ...Object.entries(r.excluded as Record<string, number>).map(([k, v]) => `excluded ${k}: ${fmt(v as number)}`),
      ...Object.entries(r.mismatches as Record<string, number>).map(([k, v]) => `mismatch ${k}: ${fmt(v as number)}`),
      ...(r.no_cached ? [`no cached value (unverifiable): ${fmt(r.no_cached)}`] : []),
    ];
    ($('receipt-detail') as HTMLElement).textContent = lines.join('\n');
  }, reduced ? 0 : 150);

  // Act 3 (findings).
  findings = a.findings as Finding[];
  setTimeout(() => {
    if (wg !== wbGen) return; // never paint a superseded file's findings
    renderAct3();
    renderChips();
    renderFindings();
    renderCapability(a.capability);
    void setupLab();
    ($('compare') as HTMLElement).hidden = false;
  }, reduced ? 0 : 300);
}

$('act2').addEventListener('click', () => {
  const d = $('receipt-detail') as HTMLElement;
  d.hidden = !d.hidden;
  $('act2').setAttribute('aria-expanded', String(!d.hidden));
});
$('act2').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    ($('act2') as HTMLElement).click();
  }
});

function visibleFindings(): Finding[] {
  return findings.filter((f) => filter === 'all' || f.detector === filter);
}

function renderAct3() {
  const act = findings.filter((f) => !suppressed.has(fkey(f))).length;
  const sup = suppressed.size;
  $('act3').innerHTML =
    act === 0
      ? `<span class="ok">0 defects found</span>` + (sup ? ` <span class="dim">· ${sup} marked intentional</span>` : '')
      : `<span class="warn">${act} defect${act === 1 ? '' : 's'} found</span>` +
        (sup ? ` <span class="dim">· ${sup} marked intentional</span>` : '');
  ($('finding-bar') as HTMLElement).hidden = findings.length === 0;
}

function renderChips() {
  const counts = new Map<string, number>();
  for (const f of findings) counts.set(f.detector, (counts.get(f.detector) ?? 0) + 1);
  const chips = [`<button class="chip${filter === 'all' ? ' active' : ''}" data-f="all">all (${findings.length})</button>`];
  for (const [d, n] of counts)
    chips.push(`<button class="chip${filter === d ? ' active' : ''}" data-f="${esc(d)}">${esc(d)} (${n})</button>`);
  $('chips').innerHTML = chips.join('');
  $('chips')
    .querySelectorAll('.chip')
    .forEach((c) =>
      c.addEventListener('click', () => {
        filter = (c as HTMLElement).dataset.f!;
        activeIdx = -1;
        renderChips();
        renderFindings();
      }),
    );
}

// Rust-style caret: point at the range the proof names; fall back to the
// first range reference in the formula.
function caretFor(f: Finding): string | null {
  const rangeRe = /\$?[A-Z]{1,3}\$?\d+(?::\$?[A-Z]{1,3}\$?\d+)?/g;
  const proofTokens = new Set(f.proof.match(rangeRe) ?? []);
  let at: { i: number; len: number } | null = null;
  for (const m of f.formula.matchAll(rangeRe)) {
    if (proofTokens.has(m[0])) {
      at = { i: m.index!, len: m[0].length };
      break;
    }
  }
  if (!at) {
    const m = f.formula.match(rangeRe);
    if (m && m.index !== undefined) at = { i: m.index, len: m[0].length };
  }
  if (!at) return null;
  // The formula line is prefixed "  --> " (6 chars); align under it.
  return ' '.repeat(6 + at.i) + '^'.repeat(at.len);
}

function renderFindings() {
  const el = $('findings');
  el.innerHTML = '';
  visibleFindings().forEach((f, i) => {
    const key = fkey(f);
    const isSup = suppressed.has(key);
    const div = document.createElement('div');
    div.className = 'finding' + (isSup ? ' suppressed' : '') + (i === activeIdx ? ' active' : '');
    const caret = caretFor(f);
    div.innerHTML =
      `<div class="head">warning[${esc(f.detector)}] <span class="loc">${esc(f.sheet)}!${esc(f.cell)}</span>` +
      `<button data-act="sup">${isSup ? 'unsuppress' : 'intentional'}</button>` +
      `<button data-act="copy" class="ghost">copy proof</button><span class="copied" hidden>copied</span></div>` +
      `<div class="formula">  --&gt; ${esc(f.formula)}</div>` +
      (caret ? `<div class="caret">${caret}</div>` : '') +
      `<div class="proof">  = proof: ${esc(f.proof)}</div>`;
    div.querySelector('[data-act="sup"]')!.addEventListener('click', () => toggleSuppress(f));
    div.querySelector('[data-act="copy"]')!.addEventListener('click', async () => {
      await navigator.clipboard.writeText(`${f.sheet}!${f.cell}: ${f.formula}\n${f.proof}`);
      const c = div.querySelector('.copied') as HTMLElement;
      c.hidden = false;
      setTimeout(() => (c.hidden = true), 1200);
    });
    el.appendChild(div);
  });
}

function toggleSuppress(f: Finding) {
  const key = fkey(f);
  if (suppressed.has(key)) suppressed.delete(key);
  else suppressed.add(key);
  saveSuppressions();
  renderAct3();
  renderFindings();
}

function renderCapability(cap: Record<string, number>) {
  const entries = Object.entries(cap).filter(([k]) => k !== 'compilable_cells');
  $('capability').textContent = entries.length
    ? 'partial compilation — excluded cells by feature:\n' + entries.map(([k, v]) => `  ${k}: ${fmt(v)}`).join('\n')
    : '';
}

// ---------- scenario lab ----------
async function setupLab() {
  if (!hasSession) return;
  const g = gen;
  let cands: typeof inputCands = [];
  try {
    cands = JSON.parse((await A('candidates', { n: 12 })).r);
  } catch {
    return;
  }
  if (g !== gen) return;
  inputCands = cands;
  if (!inputCands.length) return;
  ($('lab') as HTMLElement).hidden = false;
  ($('next-hint') as HTMLElement).hidden = false;
  // The previous workbook's readout/curve must not show while the new
  // cone prepares; sweepPts was cleared in run(), so this blanks the chart.
  ($('whatif-read') as HTMLElement).innerHTML = '<span class="dim">preparing…</span>';
  drawCurve();
  const sel = $('input-sel') as HTMLSelectElement;
  sel.innerHTML = inputCands
    .map((c, i) => `<option value="${i}">${esc(c.name)} = ${fmtV(c.value)}  (feeds ${c.impact} formulas)</option>`)
    .join('');
  sel.onchange = () => void prepareInput(inputCands[+sel.value]);
  ($('dist-sel') as HTMLSelectElement).onchange = syncDistParams;
  await prepareInput(inputCands[0]);
}

async function prepareInput(c: (typeof inputCands)[0]) {
  if (!hasSession) return;
  curInput = c;
  const g = ++gen;
  ($('cone-info') as HTMLElement).textContent = 'building the schedule…';
  const t0 = performance.now();
  let p: any;
  try {
    p = JSON.parse((await A('prepare', { sheet: c.sheet, row: c.row, col: c.col })).r);
  } catch (e) {
    p = { ok: false, error: String(e) };
  }
  if (g !== gen) return;
  if (!p.ok) {
    ($('cone-info') as HTMLElement).textContent = p.error;
    // No cone means no tornado build will be scheduled — don't leave a
    // stale "computing…" or an old chart standing.
    ($('tool-tornado') as HTMLElement).hidden = true;
    ($('tornado-status') as HTMLElement).hidden = true;
    return;
  }
  const ms = performance.now() - t0;
  ($('cone-info') as HTMLElement).textContent = `${fmt(p.cone_cells)} downstream formula${p.cone_cells === 1 ? '' : 's'} recompute (the cone) · schedule built in ${ms.toFixed(1)} ms`;
  const wsel = $('watch-sel') as HTMLSelectElement;
  wsel.innerHTML = (p.sinks as any[])
    .map((s) => `<option value="${s.sheet},${s.row},${s.col}">${esc(s.name)}</option>`)
    .join('');
  ($('monte-input') as HTMLElement).textContent = c.name;
  ($('monte-watch') as HTMLElement).textContent = wsel.selectedOptions[0]?.textContent ?? '';
  wsel.onchange = () => {
    const [sh, r, co] = wsel.value.split(',').map(Number);
    // A watch switch invalidates every in-flight continuation: a what-if
    // computed against the old output must not paint under the new
    // label, and a goal-seek bisection must not mix outputs mid-solve.
    gen++;
    // Worker A serializes its queue, so set_watch lands before any
    // later sweep/what-if without an explicit await.
    void A('set_watch', { sheet: sh, row: r, col: co });
    ($('monte-watch') as HTMLElement).textContent = wsel.selectedOptions[0]?.textContent ?? '';
    void refreshCurve();
    scheduleTornado();
  };
  ($('curve-input') as HTMLElement).textContent = c.name;
  // Slider range: current ±50% (or ±5 when near zero).
  const span = Math.max(Math.abs(c.value) * 0.5, 5);
  sliderLo = c.value - span;
  sliderHi = c.value + span;
  ($('whatif') as HTMLInputElement).value = '500';
  ($('goal-read') as HTMLElement).textContent = '';
  syncDistParams();
  await refreshCurve();
  scheduleTornado();
}

const P_LABELS: Record<string, [string, string, string]> = {
  normal: ['mean μ', 'sd σ', ''],
  uniform: ['min', 'max', ''],
  triangular: ['min', 'most likely', 'max'],
  pert: ['min', 'most likely', 'max'],
};

function syncDistParams() {
  if (!curInput) return;
  const kind = ($('dist-sel') as HTMLSelectElement).value;
  const v = curInput.value;
  const p1 = $('p1') as HTMLInputElement;
  const p2 = $('p2') as HTMLInputElement;
  const p3 = $('p3') as HTMLInputElement;
  const [l1, l2, l3] = P_LABELS[kind] ?? P_LABELS.normal;
  $('p1-lab').textContent = l1;
  $('p2-lab').textContent = l2;
  if (l3) $('p3-lab').textContent = l3;
  ($('p3-wrap') as HTMLElement).hidden = !(kind === 'triangular' || kind === 'pert');
  // ±20% around the current value; for negative values 0.8v > 1.2v, so
  // order the bounds explicitly or triangular/pert get a > b.
  const lo = Math.min(v * 0.8, v * 1.2);
  const hi = Math.max(v * 0.8, v * 1.2);
  if (kind === 'normal') {
    p1.value = String(v);
    p2.value = String(Math.abs(v) * 0.1 || 1);
  } else if (kind === 'uniform') {
    p1.value = String(lo);
    p2.value = String(hi);
  } else {
    p1.value = String(lo);
    p2.value = String(v);
    p3.value = String(hi);
  }
}

// ---------- what-if: latest-wins pipeline ----------
// At most one what_if is ever in flight; new input while waiting just
// marks the pipeline dirty and the newest slider value is sent the
// moment the previous answer lands. Nothing queues, nothing freezes.
const slider = $('whatif') as HTMLInputElement;
let whatifInflight = false;
slider.addEventListener('input', () => void pumpWhatIf());
function sliderValue(): number {
  return sliderLo + ((sliderHi - sliderLo) * +slider.value) / 1000;
}
async function pumpWhatIf() {
  if (whatifInflight || !hasSession || !curInput) return;
  whatifInflight = true;
  const g = gen;
  const x = sliderValue();
  slider.setAttribute('aria-valuetext', `${curInput.name} = ${fmtV(x)}`);
  let out: any;
  let us = 0;
  try {
    const res = await A('what_if', { x });
    out = JSON.parse(res.r);
    us = res.ms * 1000; // engine time only — queue wait would inflate it ~100x
  } catch (e) {
    out = { ok: false, error: String(e) };
  }
  whatifInflight = false;
  if (g !== gen) return;
  const watch = ($('watch-sel') as HTMLSelectElement).selectedOptions[0]?.textContent ?? '';
  ($('whatif-read') as HTMLElement).innerHTML = out.ok
    ? `${esc(curInput.name)} = ${fmtV(x)} → <span class="loc">${esc(watch)}</span> = <span class="ok">${out.output !== undefined ? fmtV(out.output) : esc(out.output_text)}</span> <span class="dim">· ${us < 1000 ? us.toFixed(0) + ' µs' : (us / 1000).toFixed(1) + ' ms'}</span>`
    : `<span class="err">${esc(out.error)}</span>`;
  drawCurve();
  if (sliderValue() !== x) void pumpWhatIf(); // newest wins
}

async function refreshCurve() {
  if (!hasSession) return;
  const g = gen;
  let pts: [number, number][] = [];
  try {
    pts = JSON.parse((await A('sweep', { lo: sliderLo, hi: sliderHi, n: 81 })).r);
  } catch {
    pts = [];
  }
  if (g !== gen) return;
  sweepPts = pts;
  sweepGen = g;
  const watch = ($('watch-sel') as HTMLSelectElement).selectedOptions[0]?.textContent ?? '';
  ($('curve-watch') as HTMLElement).textContent = watch;
  drawCurve();
  void pumpWhatIf();
}

// ---------- goal seek ----------
let goalInflight = false;
async function runGoalSeek() {
  if (goalInflight || !hasSession || !curInput) return;
  const read = $('goal-read') as HTMLElement;
  const t = parseFloat(($('goal-target') as HTMLInputElement).value);
  if (!isFinite(t)) {
    read.innerHTML = `<span class="err">enter a target number</span>`;
    return;
  }
  if (sweepPts.length < 2) {
    read.innerHTML = `<span class="err">no response curve to search</span>`;
    return;
  }
  // Bracketing on a curve from a different input/watch than the engine's
  // current state converges to garbage and reports it as "found".
  if (sweepGen !== gen) {
    read.innerHTML = `<span class="warn">the response curve is still updating — try again in a moment</span>`;
    return;
  }
  const g = gen;
  const f = async (x: number): Promise<number> => {
    const o = JSON.parse((await A('what_if', { x })).r);
    return o.ok && o.output !== undefined ? (o.output as number) : NaN;
  };
  const ys = sweepPts.map((p) => p[1]);
  // sweep() drops non-numeric points, so require the bracketing pair to be
  // adjacent on the sweep grid — never bracket across an error/text gap.
  const step = (sliderHi - sliderLo) / 80;
  let bracket: [number, number] | null = null;
  for (let i = 1; i < sweepPts.length; i++) {
    if (sweepPts[i][0] - sweepPts[i - 1][0] > step * 1.5) continue;
    if ((sweepPts[i - 1][1] - t) * (sweepPts[i][1] - t) <= 0) {
      bracket = [sweepPts[i - 1][0], sweepPts[i][0]];
      break;
    }
  }
  if (!bracket) {
    read.innerHTML =
      `<span class="warn">not reachable in the ±50% slider range</span> ` +
      `<span class="dim">(output spans ${fmtV(Math.min(...ys))} – ${fmtV(Math.max(...ys))})</span>`;
    return;
  }
  let [a, b] = bracket;
  read.innerHTML = `<span class="dim">bisecting…</span>`;
  const cancelled = () => {
    read.innerHTML = `<span class="dim">cancelled — the model changed mid-solve</span>`;
  };
  goalInflight = true;
  try {
    const t0 = performance.now();
    let fa = await f(a);
    for (let i = 0; i < 60 && Math.abs(b - a) > Math.abs(b) * 1e-13 + 1e-13; i++) {
      if (g !== gen) return cancelled(); // model switched mid-solve — drop the result
      const m = (a + b) / 2;
      const ym = await f(m);
      if (!isFinite(ym)) break;
      if ((fa - t) * (ym - t) <= 0) b = m;
      else {
        a = m;
        fa = ym;
      }
    }
    const x = (a + b) / 2;
    const y = await f(x);
    const ms = performance.now() - t0;
    if (g !== gen) return cancelled();
    if (!isFinite(y)) {
      read.innerHTML = `<span class="warn">the output is non-numeric near ${fmtV(x)}</span>`;
      return;
    }
    slider.value = String(Math.round(Math.min(1000, Math.max(0, (1000 * (x - sliderLo)) / (sliderHi - sliderLo)))));
    void pumpWhatIf();
    read.innerHTML =
      `found: <span class="loc">${esc(curInput.name)}</span> = <strong>${fmtV(x)}</strong> → ${fmtV(y)} ` +
      `<span class="dim">(target ${fmtV(t)} · bisection, ${ms.toFixed(1)} ms)</span>`;
  } catch {
    read.innerHTML = `<span class="err">the engine stopped responding — reload the workbook</span>`;
  } finally {
    goalInflight = false;
  }
}
$('goal-run').addEventListener('click', () => void runGoalSeek());
$('goal-target').addEventListener('keydown', (e) => {
  if ((e as KeyboardEvent).key === 'Enter') void runGoalSeek();
});

// ---------- tornado sensitivity (worker B: never blocks the slider) ----------
type TornadoRow = { name: string; loOut: number; hiOut: number };
let tornadoRows: TornadoRow[] = [];
let tornadoBase = 0;
let tornadoTimer = 0;

function scheduleTornado() {
  clearTimeout(tornadoTimer);
  tornadoTimer = window.setTimeout(() => void buildTornado(), 30);
}

async function buildTornado() {
  if (!hasSession || !curInput) return;
  const wsel = $('watch-sel') as HTMLSelectElement;
  if (!wsel.value) return;
  const g = gen;
  const [wsh, wr, wc] = wsel.value.split(',').map(Number);
  const tool = $('tool-tornado') as HTMLElement;
  const status = $('tornado-status') as HTMLElement;
  tool.hidden = false;
  ($('tornado-wrap') as HTMLElement).hidden = true;
  status.hidden = false;
  status.textContent = `computing ±10% swings across ${inputCands.length} assumptions…`;
  let r: any;
  try {
    r = JSON.parse((await B('tornado', { cands: inputCands, watch: { sheet: wsh, row: wr, col: wc } })).r);
  } catch {
    r = { ok: false };
  }
  if (g !== gen) return; // input/watch/workbook changed while computing
  status.hidden = true;
  tornadoBase = r.ok ? r.base : 0;
  tornadoRows = r.ok
    ? (r.rows as TornadoRow[]).sort((a, b) => Math.abs(b.hiOut - b.loOut) - Math.abs(a.hiOut - a.loOut)).slice(0, 10)
    : [];
  const wrap = $('tornado-wrap') as HTMLElement;
  wrap.hidden = tornadoRows.length < 2;
  tool.hidden = wrap.hidden;
  if (wrap.hidden) return;
  ($('tornado-watch') as HTMLElement).textContent = wsel.selectedOptions[0]?.textContent ?? '';
  $('tornado').setAttribute(
    'aria-label',
    `sensitivity, base output ${fmtV(tornadoBase)}: ` +
      tornadoRows.map((r2) => `${r2.name} −10% → ${fmtV(r2.loOut)}, +10% → ${fmtV(r2.hiOut)}`).join('; '),
  );
  drawTornado();
}

function drawTornado() {
  const cv = $('tornado') as HTMLCanvasElement;
  const rows = tornadoRows;
  if (!rows.length) return;
  const rowH = 30;
  const padT = 8;
  const padB = 26;
  cv.setAttribute('height', String(padT + rows.length * rowH + padB));
  const { ctx, w, h } = chartFrame(cv);
  const gut = Math.min(230, Math.floor(w * 0.32));
  const padR = 14;
  let vLo = Math.min(tornadoBase, ...rows.map((r) => Math.min(r.loOut, r.hiOut)));
  let vHi = Math.max(tornadoBase, ...rows.map((r) => Math.max(r.loOut, r.hiOut)));
  if (vLo === vHi) {
    vLo -= 1;
    vHi += 1;
  }
  const X = (v: number) => gut + ((w - gut - padR) * (v - vLo)) / (vHi - vLo);

  // base axis
  ctx.strokeStyle = RULE;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(X(tornadoBase), padT);
  ctx.lineTo(X(tornadoBase), h - padB + 4);
  ctx.stroke();

  const bar = (y: number, from: number, to: number, color: string) => {
    const x0 = X(from);
    const x1 = X(to);
    const left = Math.min(x0, x1);
    const bw = Math.max(1, Math.abs(x1 - x0));
    const bh = 7;
    const rr = Math.min(3, bw / 2);
    ctx.fillStyle = color;
    ctx.beginPath();
    // rounded outer end, flat end at the base axis
    if (x1 >= x0) {
      ctx.moveTo(left, y);
      ctx.lineTo(left + bw - rr, y);
      ctx.arcTo(left + bw, y, left + bw, y + rr, rr);
      ctx.lineTo(left + bw, y + bh - rr);
      ctx.arcTo(left + bw, y + bh, left + bw - rr, y + bh, rr);
      ctx.lineTo(left, y + bh);
    } else {
      ctx.moveTo(left + bw, y);
      ctx.lineTo(left + rr, y);
      ctx.arcTo(left, y, left, y + rr, rr);
      ctx.lineTo(left, y + bh - rr);
      ctx.arcTo(left, y + bh, left + rr, y + bh, rr);
      ctx.lineTo(left + bw, y + bh);
    }
    ctx.closePath();
    ctx.fill();
  };

  rows.forEach((r, i) => {
    const yTop = padT + i * rowH;
    ctx.fillStyle = DIM;
    let name = r.name;
    while (name.length > 3 && ctx.measureText(name).width > gut - 12) name = name.slice(0, -1);
    ctx.fillText(name === r.name ? name : name + '…', 0, yTop + rowH / 2 + 4);
    bar(yTop + rowH / 2 - 9, tornadoBase, r.loOut, DATA);
    bar(yTop + rowH / 2 + 2, tornadoBase, r.hiOut, DATA2);
  });

  // axis end labels + base label (text tokens)
  ctx.fillStyle = DIM;
  ctx.fillText(fmtV(vLo), gut, h - 8);
  const hiTxt = fmtV(vHi);
  ctx.fillText(hiTxt, w - padR - ctx.measureText(hiTxt).width, h - 8);
  const baseTxt = `base ${fmtV(tornadoBase)}`;
  const bx = Math.min(Math.max(X(tornadoBase) - ctx.measureText(baseTxt).width / 2, gut), w - padR - ctx.measureText(baseTxt).width);
  ctx.fillText(baseTxt, bx, h - 8);

  hoverLayer(cv, $('tornado-tip') as HTMLElement, (_px, py) => {
    const i = Math.max(0, Math.min(rows.length - 1, Math.floor((py - padT) / rowH)));
    const r = rows[i];
    return {
      x: X(tornadoBase),
      y: padT + i * rowH + 4,
      text: `${r.name}\n−10% → ${fmtV(r.loOut)} · +10% → ${fmtV(r.hiOut)}`,
    };
  });
}

// ---------- charts (validated hues, hover layer, dim grid) ----------
function chartFrame(cv: HTMLCanvasElement) {
  const ctx = cv.getContext('2d')!;
  const dpr = devicePixelRatio || 1;
  const w = cv.clientWidth || +cv.getAttribute('width')!;
  const h = +cv.getAttribute('height')!;
  // Resizing a canvas reallocates its backing store; at drag frequency
  // that reallocation is the difference between smooth and dead. Keep
  // the store and only reallocate when the size actually changed.
  const bw = Math.round(w * dpr);
  const bh = Math.round(h * dpr);
  if (cv.width !== bw || cv.height !== bh) {
    cv.width = bw;
    cv.height = bh;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  ctx.font = '11px "JetBrains Mono", monospace';
  return { ctx, w, h };
}

function grid(ctx: CanvasRenderingContext2D, w: number, h: number, pad: number) {
  ctx.strokeStyle = RULE;
  ctx.lineWidth = 1;
  for (let i = 1; i <= 3; i++) {
    const y = pad + ((h - 2 * pad) * i) / 4;
    ctx.beginPath();
    ctx.moveTo(pad, y);
    ctx.lineTo(w - pad, y);
    ctx.stroke();
  }
}

function vmark(ctx: CanvasRenderingContext2D, x: number, top: number, bot: number, label: string) {
  ctx.save();
  ctx.strokeStyle = DIM;
  ctx.lineWidth = 1;
  ctx.setLineDash([3, 3]);
  ctx.beginPath();
  ctx.moveTo(x, top);
  ctx.lineTo(x, bot);
  ctx.stroke();
  ctx.restore();
  ctx.fillStyle = DIM;
  ctx.fillText(label, x - ctx.measureText(label).width / 2, top - 2);
}

function drawCurve() {
  const cv = $('curve') as HTMLCanvasElement;
  const { ctx, w, h } = chartFrame(cv);
  if (sweepPts.length < 2) return;
  const pad = 34;
  const xs = sweepPts.map((p) => p[0]);
  const ys = sweepPts.map((p) => p[1]);
  const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
  let [y0, y1] = [Math.min(...ys), Math.max(...ys)];
  if (y0 === y1) {
    y0 -= 1;
    y1 += 1;
  }
  const X = (x: number) => pad + ((w - 2 * pad) * (x - x0)) / (x1 - x0);
  const Y = (y: number) => h - pad - ((h - 2 * pad) * (y - y0)) / (y1 - y0);
  cv.setAttribute(
    'aria-label',
    `response curve: input ${fmtV(x0)} to ${fmtV(x1)}, output ${fmtV(y0)} to ${fmtV(y1)}`,
  );
  grid(ctx, w, h, pad);
  // axis end labels (text tokens, not series color)
  ctx.fillStyle = DIM;
  ctx.fillText(fmtV(y1), 2, Y(y1) + 4);
  ctx.fillText(fmtV(y0), 2, Y(y0) + 4);
  ctx.fillText(fmtV(x0), pad, h - 6);
  const xw = ctx.measureText(fmtV(x1)).width;
  ctx.fillText(fmtV(x1), w - pad - xw, h - 6);
  // the series
  ctx.strokeStyle = DATA;
  ctx.lineWidth = 2;
  ctx.beginPath();
  sweepPts.forEach(([x, y], i) => (i ? ctx.lineTo(X(x), Y(y)) : ctx.moveTo(X(x), Y(y))));
  ctx.stroke();
  // current slider position: emphasized marker with surface ring
  const sx = sliderValue();
  const near = sweepPts.reduce((a, b) => (Math.abs(b[0] - sx) < Math.abs(a[0] - sx) ? b : a));
  ctx.beginPath();
  ctx.arc(X(near[0]), Y(near[1]), 5, 0, Math.PI * 2);
  ctx.fillStyle = DATA;
  ctx.fill();
  ctx.strokeStyle = '#0d1117';
  ctx.lineWidth = 2;
  ctx.stroke();
  hoverLayer(cv, $('curve-tip') as HTMLElement, (px) => {
    const fx = x0 + ((px - pad) / (w - 2 * pad)) * (x1 - x0);
    const p = sweepPts.reduce((a, b) => (Math.abs(b[0] - fx) < Math.abs(a[0] - fx) ? b : a));
    return { x: X(p[0]), y: Y(p[1]), text: `${fmtV(p[0])} → ${fmtV(p[1])}` };
  });
}

let histData: { bins: number[]; lo: number; width: number; n: number; p5: number; p50: number; p95: number } | null = null;
let histMode: 'hist' | 'cdf' = 'hist';

function drawHist() {
  if (!histData) return;
  const cv = $('hist') as HTMLCanvasElement;
  const { ctx, w, h } = chartFrame(cv);
  const pad = 34;
  const { bins, lo, width, n, p5, p50, p95 } = histData;
  const span = width * bins.length;
  const Xv = (v: number) => pad + ((w - 2 * pad) * (v - lo)) / span;
  cv.setAttribute(
    'aria-label',
    `${histMode === 'hist' ? 'histogram' : 'cumulative distribution'} of ${fmt(n)} scenarios, ` +
      `${fmtV(lo)} to ${fmtV(lo + span)}, p5 ${fmtV(p5)}, median ${fmtV(p50)}, p95 ${fmtV(p95)}`,
  );
  grid(ctx, w, h, pad);

  if (histMode === 'hist') {
    const maxc = Math.max(...bins);
    const bw = (w - 2 * pad) / bins.length;
    ctx.fillStyle = DATA;
    bins.forEach((c, i) => {
      if (!c) return;
      const bh = ((h - 2 * pad) * c) / maxc;
      const x = pad + i * bw + 1; // 2px surface gap between bars
      const y = h - pad - bh;
      const wpx = Math.max(1, bw - 2);
      const rr = Math.min(4, wpx / 2, bh); // rounded data-end, anchored base
      ctx.beginPath();
      ctx.moveTo(x, h - pad);
      ctx.lineTo(x, y + rr);
      ctx.arcTo(x, y, x + rr, y, rr);
      ctx.lineTo(x + wpx - rr, y);
      ctx.arcTo(x + wpx, y, x + wpx, y + rr, rr);
      ctx.lineTo(x + wpx, h - pad);
      ctx.closePath();
      ctx.fill();
    });
    hoverLayer(cv, $('hist-tip') as HTMLElement, (px) => {
      const i = Math.max(0, Math.min(bins.length - 1, Math.floor((px - pad) / bw)));
      const c = bins[i];
      const share = ((100 * c) / n).toFixed(1);
      return {
        x: pad + i * bw + bw / 2,
        y: h - pad - ((h - 2 * pad) * c) / Math.max(...bins),
        text: `${fmtV(lo + i * width)} – ${fmtV(lo + (i + 1) * width)}\n${fmt(c)} scenarios (${share}%)`,
      };
    });
  } else {
    // cumulative: share of scenarios at or below each bin edge
    const cum: number[] = [];
    let acc = 0;
    for (const c of bins) {
      acc += c;
      cum.push(acc / n);
    }
    const Y = (p: number) => h - pad - (h - 2 * pad) * p;
    ctx.strokeStyle = DATA;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(Xv(lo), Y(0));
    cum.forEach((p, i) => ctx.lineTo(Xv(lo + (i + 1) * width), Y(p)));
    ctx.stroke();
    ctx.fillStyle = DIM;
    ctx.fillText('100%', 2, Y(1) + 4);
    ctx.fillText('0%', 2, Y(0) + 4);
    hoverLayer(cv, $('hist-tip') as HTMLElement, (px) => {
      const i = Math.max(0, Math.min(bins.length - 1, Math.round((px - pad) / ((w - 2 * pad) / bins.length)) - 1));
      const v = lo + (i + 1) * width;
      return { x: Xv(v), y: Y(cum[i]), text: `≤ ${fmtV(v)}: ${(cum[i] * 100).toFixed(1)}%` };
    });
  }

  // percentile markers on both views (dim, dashed, labeled)
  vmark(ctx, Xv(p5), pad, h - pad, 'p5');
  vmark(ctx, Xv(p50), pad, h - pad, 'p50');
  vmark(ctx, Xv(p95), pad, h - pad, 'p95');
  ctx.fillStyle = DIM;
  ctx.fillText(fmtV(lo), pad, h - 6);
  const hiTxt = fmtV(lo + span);
  ctx.fillText(hiTxt, w - pad - ctx.measureText(hiTxt).width, h - 6);
}

for (const [id, mode] of [
  ['view-hist', 'hist'],
  ['view-cdf', 'cdf'],
] as const) {
  $(id).addEventListener('click', () => {
    histMode = mode;
    $('view-hist').classList.toggle('active', mode === 'hist');
    $('view-cdf').classList.toggle('active', mode === 'cdf');
    drawHist();
  });
}

function hoverLayer(
  cv: HTMLCanvasElement,
  tip: HTMLElement,
  probe: (px: number, py: number) => { x: number; y: number; text: string },
) {
  cv.onmousemove = (e) => {
    const r = cv.getBoundingClientRect();
    const { x, y, text } = probe(e.clientX - r.left, e.clientY - r.top);
    tip.hidden = false;
    tip.textContent = text;
    tip.style.left = `${x}px`;
    tip.style.top = `${Math.max(24, y)}px`;
  };
  cv.onmouseleave = () => (tip.hidden = true);
}

// ---------- monte-carlo (worker B: the slider stays live while it runs) ----------
async function runMonte() {
  if (!hasSession || !curInput) return;
  const wsel = $('watch-sel') as HTMLSelectElement;
  if (!wsel.value) return;
  const [wsh, wr, wc] = wsel.value.split(',').map(Number);
  const kind = ($('dist-sel') as HTMLSelectElement).value;
  const p1 = +($('p1') as HTMLInputElement).value;
  const p2 = +($('p2') as HTMLInputElement).value;
  const p3 = +($('p3') as HTMLInputElement).value || 0;
  const btn = $('run-monte') as HTMLButtonElement;
  const label = btn.textContent;
  btn.disabled = true;
  btn.textContent = 'running…';
  const g = gen;
  const t0 = performance.now();
  let m: any;
  try {
    m = JSON.parse(
      (
        await B('monte', {
          input: { sheet: curInput.sheet, row: curInput.row, col: curInput.col },
          watch: { sheet: wsh, row: wr, col: wc },
          kind,
          p1,
          p2,
          p3,
          n: 10_000,
        })
      ).r,
    );
  } catch (e) {
    m = { ok: false, error: String(e) };
  }
  const ms = performance.now() - t0;
  btn.disabled = false;
  btn.textContent = label;
  if (g !== gen) {
    toast('scenario run discarded — the model changed while it ran');
    return;
  }
  const out = $('monte-out') as HTMLElement;
  out.hidden = false;
  if (!m.ok) {
    $('monte-stats').innerHTML = `<span class="err">${esc(m.error)}</span>`;
    return;
  }
  $('monte-stats').innerHTML =
    `<span class="ok">${fmt(m.n)} scenarios</span> over a ${fmt(m.cone_cells)}-cell cone in <strong>${ms.toFixed(0)} ms</strong> — ` +
    `mean <strong>${fmtV(m.mean)}</strong> · sd ${fmtV(m.sd)} · p5 ${fmtV(m.p5)} · p50 ${fmtV(m.p50)} · p95 ${fmtV(m.p95)}`;
  ($('hist-watch') as HTMLElement).textContent = wsel.selectedOptions[0]?.textContent ?? '';
  ($('hist-n') as HTMLElement).textContent = fmt(m.n);
  histData = { bins: m.bins, lo: m.bin_lo, width: m.bin_width, n: m.n, p5: m.p5, p50: m.p50, p95: m.p95 };
  drawHist();
  // Worker A's prepared engine was never touched — the slider was live
  // the whole time and needs no restore.
}
$('run-monte').addEventListener('click', () => void runMonte());

// ---------- compare versions (worker B) ----------
$('pick-b').addEventListener('click', () => ($('file-b') as HTMLInputElement).click());
($('file-b') as HTMLInputElement).addEventListener('change', async () => {
  const f = ($('file-b') as HTMLInputElement).files?.[0];
  if (!f || !bytesA) return;
  // Capture the workbook this diff belongs to: a new drop mid-diff must
  // neither swap version A under us nor get the result painted under it.
  const wg = wbGen;
  const verA = bytesA;
  ($('diff-status') as HTMLElement).textContent = `running both versions over identical scenario tiles…`;
  const bb = await f.arrayBuffer();
  if (wg !== wbGen) return;
  const t0 = performance.now();
  let d: any;
  try {
    d = JSON.parse((await B('diff', { a: verA, b: bb, n: 5000 })).r);
  } catch (e) {
    d = { ok: false, error: String(e) };
  }
  if (wg !== wbGen) return; // a new workbook loaded while the diff ran
  const ms = performance.now() - t0;
  const out = $('diff-out') as HTMLElement;
  out.hidden = false;
  if (!d.ok) {
    ($('diff-status') as HTMLElement).textContent = '';
    out.innerHTML = `<span class="err">${esc(d.error)}</span>`;
    return;
  }
  ($('diff-status') as HTMLElement).textContent = `${ms.toFixed(0)} ms`;
  const pctD = ((100 * d.divergent) / d.scenarios).toFixed(1);
  if (!d.witness) {
    out.innerHTML = `<span class="ok">no divergence</span> on ${fmt(d.scenarios)} sampled scenarios at <span class="loc">${esc(d.output)}</span>`;
    return;
  }
  const w = d.witness;
  out.innerHTML =
    `<div class="act"><span class="warn">${pctD}% of the sampled input space diverges</span> at <span class="loc">${esc(d.output)}</span> (${fmt(d.divergent)}/${fmt(d.scenarios)})</div>` +
    `<div class="dim">witness — scenario ${w.scenario}:</div>` +
    `<table><tr><th>input</th><th>value</th></tr>` +
    (w.inputs as any[]).slice(0, 8).map((i) => `<tr><td class="loc">${esc(i.cell)}</td><td>${fmtV(i.value)}</td></tr>`).join('') +
    `</table>` +
    `<div>at that vector: <span class="loc">${esc(d.output)}</span> — version A = <strong>${esc(w.v1.replace(/Num\((.*)\)/, '$1'))}</strong>, version B = <strong>${esc(w.v2.replace(/Num\((.*)\)/, '$1'))}</strong></div>`;
});

// ---------- export report ----------
function exportReport() {
  if (!lastAnalysis) {
    toast('load a workbook first');
    return;
  }
  const a = lastAnalysis;
  const r = a.receipt;
  const pct = (r.rate * 100).toFixed(2);
  const L: string[] = [];
  L.push(`# xlc audit report — ${lastName}`);
  L.push('');
  L.push(`generated ${new Date().toISOString()} · entirely on this machine — nothing was uploaded`);
  L.push('');
  L.push(`## receipt`);
  L.push('');
  L.push(`- ${fmt(r.pass)}/${fmt(r.verifiable)} verifiable formula cells re-derived bit-exact (${pct}%)`);
  L.push(`- policy: bit-identical, 1 ULP, or 15 significant decimal digits`);
  L.push(`- exact ${fmt(r.pass - r.ulp1 - r.sig15)} · 1-ulp ${fmt(r.ulp1)} · 15-sig-digit ${fmt(r.sig15)}`);
  for (const [k, v] of Object.entries(r.excluded as Record<string, number>)) L.push(`- excluded ${k}: ${fmt(v)}`);
  for (const [k, v] of Object.entries(r.mismatches as Record<string, number>)) L.push(`- mismatch ${k}: ${fmt(v)}`);
  if (r.no_cached) L.push(`- no cached value (unverifiable): ${fmt(r.no_cached)}`);
  L.push('');
  const act = findings.filter((f) => !suppressed.has(fkey(f)));
  L.push(`## findings — ${act.length} defect${act.length === 1 ? '' : 's'}, ${suppressed.size} marked intentional`);
  for (const f of findings) {
    L.push('');
    L.push(`### ${suppressed.has(fkey(f)) ? '[intentional] ' : ''}warning[${f.detector}] ${f.sheet}!${f.cell}`);
    L.push('');
    L.push('```');
    L.push(f.formula);
    L.push('```');
    L.push(`proof: ${f.proof}`);
  }
  const cap = Object.entries((a.capability ?? {}) as Record<string, number>).filter(([k]) => k !== 'compilable_cells');
  if (cap.length) {
    L.push('');
    L.push(`## partial compilation — excluded cells by feature`);
    L.push('');
    for (const [k, v] of cap) L.push(`- ${k}: ${fmt(v)}`);
  }
  if (histData || tornadoRows.length) {
    L.push('');
    L.push(`## scenario lab`);
    if (histData) {
      const s = $('monte-stats').textContent ?? '';
      L.push('');
      L.push(`- distribution of ${$('hist-watch').textContent}: ${s.trim()}`);
      L.push(`- deterministic: seed 42 — scenario k is derivable from (seed, k)`);
    }
    if (tornadoRows.length) {
      L.push('');
      L.push(`### sensitivity — swing of ${$('tornado-watch').textContent} (each assumption ±10%, base ${fmtV(tornadoBase)})`);
      L.push('');
      L.push(`| assumption | −10% → | +10% → |`);
      L.push(`|---|---|---|`);
      for (const t of tornadoRows) L.push(`| ${t.name} | ${fmtV(t.loOut)} | ${fmtV(t.hiOut)} |`);
    }
  }
  L.push('');
  L.push(`---`);
  L.push(`produced by xlc — https://github.com/vineetsista/xlc`);
  const blob = new Blob([L.join('\n')], { type: 'text/markdown' });
  const url = URL.createObjectURL(blob);
  const aEl = document.createElement('a');
  aEl.href = url;
  aEl.download = `${lastName.replace(/\.(xlsx|xlsm)$/i, '')}-xlc-report.md`;
  document.body.appendChild(aEl);
  aEl.click();
  aEl.remove();
  URL.revokeObjectURL(url);
  toast('report downloaded');
}
$('export-report').addEventListener('click', exportReport);

async function copyAllProofs() {
  const vis = visibleFindings();
  if (!vis.length) {
    toast('no findings to copy');
    return;
  }
  await navigator.clipboard.writeText(vis.map((f) => `${f.sheet}!${f.cell}: ${f.formula}\n${f.proof}`).join('\n\n'));
  toast(`${vis.length} proof${vis.length === 1 ? '' : 's'} copied`);
}

// ---------- sticky nav + scrollspy ----------
const jump = (sel: string) => {
  const el = document.querySelector<HTMLElement>(sel);
  el?.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth' });
  // .tool targets carry tabindex="-1" so palette jumps move focus too
  if (el?.getAttribute('tabindex') === '-1') el.focus({ preventScroll: true });
};
document.querySelectorAll('#topnav a').forEach((a) =>
  a.addEventListener('click', (e) => {
    e.preventDefault();
    jump((a as HTMLAnchorElement).getAttribute('href')!);
  }),
);
let spyTick = false;
document.addEventListener(
  'scroll',
  () => {
    if (spyTick) return;
    spyTick = true;
    requestAnimationFrame(() => {
      spyTick = false;
      const links = [...document.querySelectorAll<HTMLAnchorElement>('#topnav a')];
      let current = links[0];
      for (const a of links) {
        const el = document.getElementById(a.dataset.spy!);
        if (el && !el.hidden && el.getBoundingClientRect().top <= 72) current = a;
      }
      links.forEach((a) => a.classList.toggle('active', a === current));
    });
  },
  { passive: true },
);

// ---------- command palette ----------
type Cmd = { name: string; hint?: string; when?: () => boolean; run: () => void };
const loaded = () => !!lastAnalysis;
const commands: Cmd[] = [
  { name: 'audit the sample budget', hint: 'demo', run: () => ($('try-sample') as HTMLElement).click() },
  { name: 'open a workbook…', hint: 'file', run: () => fileInput.click() },
  { name: 'export audit report', hint: '.md download', when: loaded, run: exportReport },
  { name: 'copy all proofs', hint: 'clipboard', when: () => findings.length > 0, run: copyAllProofs },
  { name: 'run 10,000 scenarios', hint: 'monte-carlo', when: () => !($('lab') as HTMLElement).hidden, run: () => void runMonte() },
  { name: 'toggle receipt breakdown', when: loaded, run: () => ($('act2') as HTMLElement).click() },
  { name: 'go to audit', when: loaded, run: () => jump('#log') },
  { name: 'goal seek: solve for an input', when: () => !($('lab') as HTMLElement).hidden, run: () => ($('goal-target') as HTMLElement).focus() },
  { name: 'go to findings', when: () => findings.length > 0, run: () => jump('#findings') },
  { name: 'go to scenario lab', when: () => !($('lab') as HTMLElement).hidden, run: () => jump('#lab') },
  { name: 'go to sensitivity', when: () => !($('tool-tornado') as HTMLElement).hidden, run: () => jump('#tool-tornado') },
  { name: 'go to monte-carlo', when: () => !($('lab') as HTMLElement).hidden, run: () => jump('#tool-monte') },
  { name: 'go to compare versions', when: () => !($('compare') as HTMLElement).hidden, run: () => jump('#compare') },
  { name: 'compare with another version…', when: loaded, run: () => ($('file-b') as HTMLInputElement).click() },
  { name: 'keyboard shortcuts', hint: '?', run: () => showOverlay('help-ov') },
];
let palIdx = 0;

function paletteMatches(): Cmd[] {
  const q = ($('palette-q') as HTMLInputElement).value.trim().toLowerCase();
  return commands.filter((c) => (!c.when || c.when()) && (!q || c.name.toLowerCase().includes(q)));
}

function renderPalette() {
  const list = $('palette-list');
  const m = paletteMatches();
  palIdx = Math.max(0, Math.min(palIdx, m.length - 1));
  list.innerHTML = m.length
    ? m
        .map(
          (c, i) =>
            `<div class="cmd${i === palIdx ? ' sel' : ''}" data-i="${i}" id="cmd-${i}" role="option" aria-selected="${i === palIdx}">${esc(c.name)}${c.hint ? `<span class="hint dim">${esc(c.hint)}</span>` : ''}</div>`,
        )
        .join('')
    : `<div class="cmd dim" role="option" aria-selected="false">no matching command</div>`;
  ($('palette-q') as HTMLInputElement).setAttribute('aria-activedescendant', m.length ? `cmd-${palIdx}` : '');
  list.querySelectorAll<HTMLElement>('.cmd[data-i]').forEach((el) =>
    el.addEventListener('click', () => {
      const c = paletteMatches()[+el.dataset.i!];
      closeOverlays();
      c?.run();
    }),
  );
}

let lastFocus: HTMLElement | null = null;
function showOverlay(id: 'palette-ov' | 'help-ov') {
  const invoker = (document.activeElement as HTMLElement) ?? null;
  closeOverlays();
  lastFocus = invoker;
  $(id).hidden = false;
  if (id === 'palette-ov') {
    const q = $('palette-q') as HTMLInputElement;
    q.value = '';
    palIdx = 0;
    renderPalette();
    q.focus();
  } else {
    ($('help') as HTMLElement).focus();
  }
}
function closeOverlays() {
  const wasOpen = overlaysOpen();
  $('palette-ov').hidden = true;
  $('help-ov').hidden = true;
  if (wasOpen) {
    // return focus to the invoking element so keyboard users keep their place
    if (lastFocus?.isConnected) lastFocus.focus();
    else ($('palette-q') as HTMLInputElement).blur();
    lastFocus = null;
  }
}
const overlaysOpen = () => !$('palette-ov').hidden || !$('help-ov').hidden;

for (const id of ['palette-ov', 'help-ov'] as const) {
  $(id).addEventListener('click', (e) => {
    if (e.target === $(id)) closeOverlays();
  });
  // keep Tab inside the open dialog
  $(id).addEventListener('keydown', (e) => {
    if ((e as KeyboardEvent).key !== 'Tab') return;
    const panel = $(id).querySelector('.panel') as HTMLElement;
    const foci = [...panel.querySelectorAll<HTMLElement>('input, button, a[href]')];
    if (!foci.length) {
      e.preventDefault();
      return;
    }
    const first = foci[0];
    const last = foci[foci.length - 1];
    if ((e as KeyboardEvent).shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!(e as KeyboardEvent).shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    } else if (!panel.contains(document.activeElement)) {
      e.preventDefault();
      first.focus();
    }
  });
}

$('palette-q').addEventListener('input', () => {
  palIdx = 0;
  renderPalette();
});
$('palette-q').addEventListener('keydown', (e) => {
  const k = (e as KeyboardEvent).key;
  if (k === 'ArrowDown') {
    e.preventDefault();
    palIdx++;
    renderPalette();
  } else if (k === 'ArrowUp') {
    e.preventDefault();
    palIdx--;
    renderPalette();
  } else if (k === 'Enter') {
    const c = paletteMatches()[palIdx];
    closeOverlays();
    c?.run();
  }
});

// ---------- global keyboard ----------
document.addEventListener('keydown', (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
    e.preventDefault();
    if (!$('palette-ov').hidden) closeOverlays();
    else showOverlay('palette-ov');
    return;
  }
  if (e.key === 'Escape') {
    closeOverlays();
    return;
  }
  if (overlaysOpen()) return;
  if (['INPUT', 'SELECT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName)) return;
  if (e.key === '?') {
    showOverlay('help-ov');
    return;
  }
  const vis = visibleFindings();
  if (!vis.length) return;
  if (e.key === 'j') activeIdx = Math.min(vis.length - 1, activeIdx + 1);
  else if (e.key === 'k') activeIdx = Math.max(0, activeIdx - 1);
  else if (e.key === 'x' && activeIdx >= 0) return toggleSuppress(vis[activeIdx]);
  else if (e.key === 'c' && activeIdx >= 0) {
    const f = vis[activeIdx];
    navigator.clipboard.writeText(`${f.sheet}!${f.cell}: ${f.formula}\n${f.proof}`);
    return;
  } else return;
  renderFindings();
  document.querySelector('.finding.active')?.scrollIntoView({ block: 'nearest' });
});
