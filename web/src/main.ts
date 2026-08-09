// xlc web surface v2: three acts + the scenario lab (live what-if slider,
// in-browser Monte-Carlo, version diff). Everything local (Law 1).
// Charts follow the dataviz method: single series -> one validated mark
// hue (--data), no legend, hover layer always, recessive grid, text in
// text tokens.

import init, { Session, diff_books } from '../pkg/xlc_wasm.js';
import { SAMPLE_B64, SAMPLE_NAME } from './sample';

type Finding = { detector: string; sheet: string; cell: string; formula: string; proof: string };

const $ = (id: string) => document.getElementById(id)!;
const wasmReady = init();
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

const DATA = '#2f81f7';
const DIM = '#7d8590';
const RULE = '#21262d';
const FG = '#e6edf3';

let session: Session | null = null;
let bytesA: ArrayBuffer | null = null;
let wbHash = '';
let findings: Finding[] = [];
let suppressed = new Set<string>();
let activeIdx = -1;
let filter = 'all';
let inputCands: { sheet: number; row: number; col: number; name: string; value: number; impact: number }[] = [];
let curInput: (typeof inputCands)[0] | null = null;
let sweepPts: [number, number][] = [];
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

function loadSuppressions(): Set<string> {
  try {
    const raw = localStorage.getItem(`xlc-suppress-${wbHash}`);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}
const saveSuppressions = () => localStorage.setItem(`xlc-suppress-${wbHash}`, JSON.stringify([...suppressed]));

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
drop.addEventListener('drop', (e) => {
  e.preventDefault();
  drop.classList.remove('over');
  const f = e.dataTransfer?.files?.[0];
  if (f) f.arrayBuffer().then((b) => run(b, f.name));
});
fileInput.addEventListener('change', () => {
  const f = fileInput.files?.[0];
  if (f) f.arrayBuffer().then((b) => run(b, f.name));
});
$('try-sample').addEventListener('click', (e) => {
  e.stopPropagation();
  const bin = atob(SAMPLE_B64);
  const buf = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) buf[i] = bin.charCodeAt(i);
  run(buf.buffer, SAMPLE_NAME);
});

// ---------- the three acts ----------
async function run(bytes: ArrayBuffer, name: string) {
  await wasmReady;
  bytesA = bytes;
  wbHash = await sha256hex(bytes);
  suppressed = loadSuppressions();
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
  ($('diff-out') as HTMLElement).hidden = true;
  ($('diff-status') as HTMLElement).textContent = '';

  const t0 = performance.now();
  try {
    session?.free();
  } catch {}
  session = null;
  let a: any;
  try {
    session = new Session(new Uint8Array(bytes));
    a = JSON.parse(session.analyze());
  } catch (e) {
    $('act1').innerHTML = `<span class="err">error:</span> ${esc(String(e))}`;
    return;
  }
  const secs = (performance.now() - t0) / 1000;
  if (!a.ok) {
    $('act1').innerHTML = `<span class="err">error:</span> ${esc(a.error ?? 'unknown')}`;
    return;
  }

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
    $('act2').innerHTML =
      `receipt: <span class="${cls}">${fmt(r.pass)}/${fmt(r.verifiable)} verifiable cells re-derived bit-exact (${pct}%)</span>` +
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
    renderAct3();
    renderChips();
    renderFindings();
    renderCapability(a.capability);
    setupLab();
    ($('compare') as HTMLElement).hidden = false;
  }, reduced ? 0 : 300);
}

$('act2').addEventListener('click', () => {
  const d = $('receipt-detail') as HTMLElement;
  d.hidden = !d.hidden;
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

function renderFindings() {
  const el = $('findings');
  el.innerHTML = '';
  visibleFindings().forEach((f, i) => {
    const key = fkey(f);
    const isSup = suppressed.has(key);
    const div = document.createElement('div');
    div.className = 'finding' + (isSup ? ' suppressed' : '') + (i === activeIdx ? ' active' : '');
    div.innerHTML =
      `<div class="head">warning[${esc(f.detector)}] <span class="loc">${esc(f.sheet)}!${esc(f.cell)}</span>` +
      `<button data-act="sup">${isSup ? 'unsuppress' : 'intentional'}</button>` +
      `<button data-act="copy" class="ghost">copy proof</button><span class="copied" hidden>copied</span></div>` +
      `<div class="formula">  --&gt; ${esc(f.formula)}</div>` +
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

document.addEventListener('keydown', (e) => {
  if (['INPUT', 'SELECT', 'TEXTAREA'].includes((e.target as HTMLElement).tagName)) return;
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

function renderCapability(cap: Record<string, number>) {
  const entries = Object.entries(cap).filter(([k]) => k !== 'compilable_cells');
  $('capability').textContent = entries.length
    ? 'partial compilation — excluded cells by feature:\n' + entries.map(([k, v]) => `  ${k}: ${fmt(v)}`).join('\n')
    : '';
}

// ---------- scenario lab ----------
function setupLab() {
  if (!session) return;
  inputCands = JSON.parse(session.input_candidates(12));
  if (!inputCands.length) return;
  ($('lab') as HTMLElement).hidden = false;
  const sel = $('input-sel') as HTMLSelectElement;
  sel.innerHTML = inputCands
    .map((c, i) => `<option value="${i}">${esc(c.name)} = ${fmtV(c.value)}  (feeds ${c.impact} formulas)</option>`)
    .join('');
  sel.onchange = () => prepareInput(inputCands[+sel.value]);
  prepareInput(inputCands[0]);
  ($('dist-sel') as HTMLSelectElement).onchange = syncDistParams;
  $('run-monte').addEventListener('click', runMonte);
}

function prepareInput(c: (typeof inputCands)[0]) {
  if (!session) return;
  curInput = c;
  const t0 = performance.now();
  const p = JSON.parse(session.prepare(c.sheet, c.row, c.col));
  if (!p.ok) {
    ($('cone-info') as HTMLElement).textContent = p.error;
    return;
  }
  const ms = performance.now() - t0;
  ($('cone-info') as HTMLElement).textContent = `cone: ${fmt(p.cone_cells)} formula${p.cone_cells === 1 ? '' : 's'} · schedule built in ${ms.toFixed(1)} ms`;
  const wsel = $('watch-sel') as HTMLSelectElement;
  wsel.innerHTML = (p.sinks as any[])
    .map((s) => `<option value="${s.sheet},${s.row},${s.col}">${esc(s.name)}</option>`)
    .join('');
  wsel.onchange = () => {
    const [sh, r, co] = wsel.value.split(',').map(Number);
    session!.set_watch(sh, r, co);
    refreshCurve();
  };
  ($('curve-input') as HTMLElement).textContent = c.name;
  // Slider range: current ±50% (or ±5 when near zero).
  const span = Math.max(Math.abs(c.value) * 0.5, 5);
  sliderLo = c.value - span;
  sliderHi = c.value + span;
  ($('whatif') as HTMLInputElement).value = '500';
  syncDistParams();
  refreshCurve();
  onSlider();
}

function syncDistParams() {
  if (!curInput) return;
  const kind = ($('dist-sel') as HTMLSelectElement).value;
  const v = curInput.value;
  const p1 = $('p1') as HTMLInputElement;
  const p2 = $('p2') as HTMLInputElement;
  const p3 = $('p3') as HTMLInputElement;
  p3.hidden = !(kind === 'triangular' || kind === 'pert');
  if (kind === 'normal') {
    p1.value = String(v);
    p2.value = String(Math.abs(v) * 0.1 || 1);
  } else if (kind === 'uniform') {
    p1.value = String(v * 0.8);
    p2.value = String(v * 1.2);
  } else {
    p1.value = String(v * 0.8);
    p2.value = String(v);
    p3.value = String(v * 1.2);
  }
}

const slider = $('whatif') as HTMLInputElement;
slider.addEventListener('input', onSlider);
function sliderValue(): number {
  return sliderLo + ((sliderHi - sliderLo) * +slider.value) / 1000;
}
function onSlider() {
  if (!session || !curInput) return;
  const x = sliderValue();
  const t0 = performance.now();
  const out = JSON.parse(session.what_if(x));
  const us = (performance.now() - t0) * 1000;
  const watch = ($('watch-sel') as HTMLSelectElement).selectedOptions[0]?.textContent ?? '';
  ($('whatif-read') as HTMLElement).innerHTML = out.ok
    ? `${esc(curInput.name)} = ${fmtV(x)} → <span class="loc">${esc(watch)}</span> = <span class="ok">${out.output !== undefined ? fmtV(out.output) : esc(out.output_text)}</span> <span class="dim">· ${us < 1000 ? us.toFixed(0) + ' µs' : (us / 1000).toFixed(1) + ' ms'}</span>`
    : `<span class="err">${esc(out.error)}</span>`;
  drawCurve();
}

function refreshCurve() {
  if (!session) return;
  sweepPts = JSON.parse(session.sweep(sliderLo, sliderHi, 81));
  const watch = ($('watch-sel') as HTMLSelectElement).selectedOptions[0]?.textContent ?? '';
  ($('curve-watch') as HTMLElement).textContent = watch;
  drawCurve();
  onSlider();
}

// ---------- charts (single series, --data hue, hover layer, dim grid) ----------
function chartFrame(cv: HTMLCanvasElement) {
  const ctx = cv.getContext('2d')!;
  const dpr = devicePixelRatio || 1;
  const w = cv.clientWidth || cv.width;
  const h = +cv.getAttribute('height')!;
  cv.width = w * dpr;
  cv.height = h * dpr;
  ctx.scale(dpr, dpr);
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

let histData: { bins: number[]; lo: number; width: number; n: number } | null = null;
function drawHist() {
  if (!histData) return;
  const cv = $('hist') as HTMLCanvasElement;
  const { ctx, w, h } = chartFrame(cv);
  const pad = 34;
  const { bins, lo, width } = histData;
  const maxc = Math.max(...bins);
  const bw = (w - 2 * pad) / bins.length;
  grid(ctx, w, h, pad);
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
  ctx.fillStyle = DIM;
  ctx.fillText(fmtV(lo), pad, h - 6);
  const hiTxt = fmtV(lo + width * bins.length);
  ctx.fillText(hiTxt, w - pad - ctx.measureText(hiTxt).width, h - 6);
  hoverLayer(cv, $('hist-tip') as HTMLElement, (px) => {
    const i = Math.max(0, Math.min(bins.length - 1, Math.floor((px - pad) / bw)));
    const c = bins[i];
    const share = ((100 * c) / histData!.n).toFixed(1);
    return {
      x: pad + i * bw + bw / 2,
      y: h - pad - ((h - 2 * pad) * c) / maxc,
      text: `${fmtV(lo + i * width)} – ${fmtV(lo + (i + 1) * width)}\n${fmt(c)} scenarios (${share}%)`,
    };
  });
}

function hoverLayer(
  cv: HTMLCanvasElement,
  tip: HTMLElement,
  probe: (px: number) => { x: number; y: number; text: string },
) {
  cv.onmousemove = (e) => {
    const r = cv.getBoundingClientRect();
    const { x, y, text } = probe(e.clientX - r.left);
    tip.hidden = false;
    tip.textContent = text;
    tip.style.left = `${x}px`;
    tip.style.top = `${Math.max(24, y)}px`;
  };
  cv.onmouseleave = () => (tip.hidden = true);
}

function runMonte() {
  if (!session || !curInput) return;
  const kind = ($('dist-sel') as HTMLSelectElement).value;
  const p1 = +($('p1') as HTMLInputElement).value;
  const p2 = +($('p2') as HTMLInputElement).value;
  const p3 = +($('p3') as HTMLInputElement).value || 0;
  const t0 = performance.now();
  const m = JSON.parse(session.monte(kind, p1, p2, p3, 10_000));
  const ms = performance.now() - t0;
  const out = $('monte-out') as HTMLElement;
  out.hidden = false;
  if (!m.ok) {
    $('monte-stats').innerHTML = `<span class="err">${esc(m.error)}</span>`;
    return;
  }
  $('monte-stats').innerHTML =
    `<span class="ok">${fmt(m.n)} scenarios</span> over a ${fmt(m.cone_cells)}-cell cone in <strong>${ms.toFixed(0)} ms</strong> — ` +
    `mean <strong>${fmtV(m.mean)}</strong> · sd ${fmtV(m.sd)} · p5 ${fmtV(m.p5)} · p50 ${fmtV(m.p50)} · p95 ${fmtV(m.p95)}`;
  ($('hist-watch') as HTMLElement).textContent = ($('watch-sel') as HTMLSelectElement).selectedOptions[0]?.textContent ?? '';
  ($('hist-n') as HTMLElement).textContent = fmt(m.n);
  histData = { bins: m.bins, lo: m.bin_lo, width: m.bin_width, n: m.n };
  drawHist();
  // The what-if engine was rebuilt with the sampling distribution; restore
  // the point engine so the slider stays live.
  const c = curInput;
  const keep = ($('watch-sel') as HTMLSelectElement).value;
  session.prepare(c.sheet, c.row, c.col);
  if (keep) {
    const [sh, r2, co] = keep.split(',').map(Number);
    session.set_watch(sh, r2, co);
  }
  onSlider();
}

// ---------- compare versions ----------
$('pick-b').addEventListener('click', () => ($('file-b') as HTMLInputElement).click());
($('file-b') as HTMLInputElement).addEventListener('change', async () => {
  const f = ($('file-b') as HTMLInputElement).files?.[0];
  if (!f || !bytesA) return;
  ($('diff-status') as HTMLElement).textContent = `running both versions over identical scenario tiles…`;
  await new Promise((r) => setTimeout(r, 30)); // let the status paint
  const bb = await f.arrayBuffer();
  const t0 = performance.now();
  const d = JSON.parse(diff_books(new Uint8Array(bytesA), new Uint8Array(bb), 5000));
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
