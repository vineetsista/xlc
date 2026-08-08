// xlc web surface: the three-act sequence (§8, Phase 4).
// Act 1: compiled N formulas · Act 2: the receipt turns green ·
// Act 3: N defects, each with its proof. Everything local (Law 1).

import init, { analyze } from '../pkg/xlc_wasm.js';

type Finding = {
  detector: string;
  sheet: string;
  cell: string;
  formula: string;
  proof: string;
};

type Analysis = {
  ok: boolean;
  error?: string;
  sheets: string[];
  formula_cells: number;
  receipt: {
    cells: number;
    pass: number;
    verifiable: number;
    rate: number;
    excluded: Record<string, number>;
    mismatches: Record<string, number>;
    no_cached: number;
  };
  findings: Finding[];
  capability: Record<string, number>;
};

const $ = (id: string) => document.getElementById(id)!;
const drop = $('drop');
const fileInput = $('file') as HTMLInputElement;

const wasmReady = init();

drop.addEventListener('click', () => fileInput.click());
drop.addEventListener('dragover', (e) => {
  e.preventDefault();
  drop.classList.add('over');
});
drop.addEventListener('dragleave', () => drop.classList.remove('over'));
drop.addEventListener('drop', (e) => {
  e.preventDefault();
  drop.classList.remove('over');
  const f = e.dataTransfer?.files?.[0];
  if (f) run(f);
});
fileInput.addEventListener('change', () => {
  const f = fileInput.files?.[0];
  if (f) run(f);
});

async function sha256hex(bytes: ArrayBuffer): Promise<string> {
  const d = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

function findingKey(f: Finding): string {
  return `${f.detector}|${f.sheet}|${f.cell}`;
}

function loadSuppressions(wbHash: string): Set<string> {
  try {
    const raw = localStorage.getItem(`xlc-suppress-${wbHash}`);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

function saveSuppressions(wbHash: string, s: Set<string>) {
  localStorage.setItem(`xlc-suppress-${wbHash}`, JSON.stringify([...s]));
}

async function run(file: File) {
  const log = $('log');
  log.hidden = false;
  const act1 = $('act1');
  const act2 = $('act2');
  const act3 = $('act3');
  const findingsEl = $('findings');
  const capEl = $('capability');
  act1.textContent = `compiling ${file.name}…`;
  act2.textContent = '';
  act3.textContent = '';
  findingsEl.textContent = '';
  capEl.textContent = '';

  await wasmReady;
  const bytes = await file.arrayBuffer();
  const wbHash = await sha256hex(bytes);
  const t0 = performance.now();
  const raw = analyze(new Uint8Array(bytes));
  const elapsed = (performance.now() - t0) / 1000;
  const a = JSON.parse(raw) as Analysis;

  if (!a.ok) {
    act1.innerHTML = `<span class="err">error:</span> ${escapeHtml(a.error ?? 'unknown')}`;
    return;
  }

  // Act 1 — compiled.
  act1.innerHTML = `compiled <strong>${fmt(a.formula_cells)}</strong> formulas across ${a.sheets.length} sheet${a.sheets.length === 1 ? '' : 's'} in <strong>${elapsed.toFixed(2)}s</strong>`;

  // Act 2 — the receipt.
  const r = a.receipt;
  const pct = (r.rate * 100).toFixed(2);
  const excludedTotal = Object.values(r.excluded).reduce((x, y) => x + y, 0);
  const cls = r.rate >= 0.97 ? 'ok' : r.rate >= 0.8 ? 'warn' : 'err';
  act2.innerHTML = `receipt: <span class="${cls}">${fmt(r.pass)}/${fmt(r.verifiable)} verifiable cells re-derived bit-exact (${pct}%)</span>` +
    (excludedTotal > 0 ? ` <span class="dim">· ${fmt(excludedTotal)} excluded (listed below)</span>` : '');

  // Act 3 — findings.
  const suppressed = loadSuppressions(wbHash);
  const active = a.findings.filter((f) => !suppressed.has(findingKey(f)));
  const n = active.length;
  act3.innerHTML =
    n === 0
      ? `<span class="ok">0 defects found</span>`
      : `<span class="warn">${n} defect${n === 1 ? '' : 's'} found</span>` +
        (suppressed.size > 0 ? ` <span class="dim">· ${suppressed.size} marked intentional</span>` : '');

  renderFindings(a.findings, wbHash, suppressed);

  // Capability report (partial compilation, Law 9).
  const caps = Object.entries(a.capability).filter(([k]) => k !== 'compilable_cells');
  if (caps.length > 0) {
    capEl.textContent =
      'partial compilation — excluded cells by feature:\n' +
      caps.map(([k, v]) => `  ${k}: ${fmt(v)}`).join('\n');
  }
}

function renderFindings(findings: Finding[], wbHash: string, suppressed: Set<string>) {
  const el = $('findings');
  el.innerHTML = '';
  for (const f of findings) {
    const key = findingKey(f);
    const isSup = suppressed.has(key);
    const div = document.createElement('div');
    div.className = 'finding' + (isSup ? ' suppressed' : '');
    div.innerHTML =
      `<div class="head">warning[${escapeHtml(f.detector)}] <span class="loc">${escapeHtml(f.sheet)}!${escapeHtml(f.cell)}</span>` +
      `<button data-key="${escapeHtml(key)}">${isSup ? 'unsuppress' : 'intentional'}</button></div>` +
      `<div class="formula">  --&gt; ${escapeHtml(f.formula)}</div>` +
      `<div class="proof">  = proof: ${escapeHtml(f.proof)}</div>`;
    div.querySelector('button')!.addEventListener('click', () => {
      if (suppressed.has(key)) suppressed.delete(key);
      else suppressed.add(key);
      saveSuppressions(wbHash, suppressed);
      renderFindings(findings, wbHash, suppressed);
      const act3 = $('act3');
      const n = findings.filter((x) => !suppressed.has(findingKey(x))).length;
      act3.innerHTML =
        n === 0
          ? `<span class="ok">0 defects found</span>` +
            (suppressed.size > 0 ? ` <span class="dim">· ${suppressed.size} marked intentional</span>` : '')
          : `<span class="warn">${n} defect${n === 1 ? '' : 's'} found</span>` +
            (suppressed.size > 0 ? ` <span class="dim">· ${suppressed.size} marked intentional</span>` : '');
    });
    el.appendChild(div);
  }
}

function fmt(n: number): string {
  return n.toLocaleString('en-US');
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
