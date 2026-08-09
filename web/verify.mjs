// Real-browser verification of the web surface: three acts, findings
// triage (chips, copy, suppression, keyboard), the v4 staged anatomy
// (stage headers, assumption→output context row, grouped tools, labeled
// distribution params), the scenario lab (slider, response curve,
// Monte-Carlo histogram), and version diff.
import { chromium } from 'playwright';
import { preview } from 'vite';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const root = path.dirname(fileURLToPath(import.meta.url));
const coop = process.argv.includes('--coop');
const server = await preview({
  root,
  preview: {
    port: 4173,
    strictPort: true,
    headers: coop
      ? {
          'Cross-Origin-Opener-Policy': 'same-origin',
          'Cross-Origin-Embedder-Policy': 'require-corp',
        }
      : {},
  },
});
const url = 'http://localhost:4173/';

const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
page.on('console', (m) => {
  if (m.type() === 'error') errors.push(m.text());
});

let failures = 0;
let checks = 0;
const check = (name, cond) => {
  checks++;
  console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}`);
  if (!cond) failures++;
};

await page.goto(url);
check('page title', (await page.title()).includes('xlc'));
check('drop zone visible', await page.locator('#drop').isVisible());

// --- one-click sample ---
await page.locator('#try-sample').click();
await page.waitForFunction(() => /defect/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 30000 });
check('sample: act1 compiled 29', /compiled 29 formulas/.test(await page.locator('#act1').textContent()));
check('sample: receipt 100% green', await page.locator('#act2 .ok').count() === 1);
check('sample: 2 defects', /2 defects found/.test(await page.locator('#act3').textContent()));

// --- receipt expandable ---
await page.locator('#act2').click();
check('receipt detail expands', await page.locator('#receipt-detail').isVisible());
check('receipt detail shows exact split', /exact 29/.test(await page.locator('#receipt-detail').textContent()));

// --- chips filter ---
check('chips render with counts', /all \(2\)/.test(await page.locator('#chips').textContent()));
await page.locator('.chip[data-f="range-off-by-one"]').click();
check('filter narrows to 1 finding', (await page.locator('.finding').count()) === 1);

// stale-filter regression: a detector filter must not survive a new load
await page.locator('#try-sample').click();
await page.waitForFunction(() => /defect/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 30000 });
check('detector filter resets on a new load', (await page.locator('.finding').count()) === 2 && await page.locator('.chip[data-f="all"]').evaluate((el) => el.classList.contains('active')));

// --- copy proof ---
await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
await page.locator('.finding [data-act="copy"]').first().click();
const clip = await page.evaluate(() => navigator.clipboard.readText());
check('copy proof puts evidence on clipboard', /proof|pattern/.test(clip) || clip.includes('!'));

// --- keyboard triage: j selects, x suppresses ---
await page.keyboard.press('j');
check('j activates first finding', (await page.locator('.finding.active').count()) === 1);
await page.keyboard.press('x');
check('x suppresses active finding', /1 defect found.*1 marked intentional/.test(await page.locator('#act3').textContent()));
await page.keyboard.press('x'); // restore
check('x again unsuppresses', /2 defects found/.test(await page.locator('#act3').textContent()));

// --- suppression persistence across reload ---
await page.locator('.finding', { hasText: 'Budget!G7' }).locator('[data-act="sup"]').click();
await page.reload();
await page.locator('#try-sample').click();
await page.waitForFunction(() => /marked intentional/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 30000 });
check('suppression survives reload + re-drop', /1 defect found.*1 marked intentional/.test(await page.locator('#act3').textContent()));
await page.locator('.finding', { hasText: 'Budget!G7' }).locator('[data-act="sup"]').click(); // restore

// --- scenario lab (engine runs in a worker: wait for async results) ---
check('lab appears', await page.locator('#lab').isVisible());
check('input candidates listed', (await page.locator('#input-sel option').count()) > 3);
await page.waitForFunction(() => /schedule built/.test(document.getElementById('cone-info')?.textContent ?? ''), null, { timeout: 30000 });
check('cone info shows schedule build', /downstream formulas? recompute \(the cone\) · schedule built/.test(await page.locator('#cone-info').textContent()));

// --- v4: staged anatomy + grouped tools ---
check('three stage headers (§1 audit, §2 lab, §3 compare)', (await page.locator('h2.stage').count()) === 3);
check('nav labels the audit stage', (await page.locator('#topnav a[data-spy="log"]').textContent()) === 'audit');
check('context row pairs assumption with output', (await page.locator('.ctx #input-sel').count()) === 1 && (await page.locator('.ctx #watch-sel').count()) === 1);
check('lab groups exactly three tools', (await page.locator('#lab .tool').count()) === 3);
check('next-hint guides from audit into the lab', await page.locator('#next-hint').isVisible());
check('normal params labeled mean/sd', (await page.locator('#p1-lab').textContent()) === 'mean μ' && (await page.locator('#p2-lab').textContent()) === 'sd σ');
await page.selectOption('#dist-sel', 'uniform');
check('uniform relabels to min/max live', (await page.locator('#p1-lab').textContent()) === 'min' && (await page.locator('#p2-lab').textContent()) === 'max' && await page.locator('#p3-wrap').isHidden());
await page.selectOption('#dist-sel', 'triangular');
check('triangular relabels to min/most likely/max', (await page.locator('#p2-lab').textContent()) === 'most likely' && await page.locator('#p3-wrap').isVisible());
await page.selectOption('#dist-sel', 'normal');
check('normal relabels back and hides the third param', (await page.locator('#p1-lab').textContent()) === 'mean μ' && await page.locator('#p3-wrap').isHidden());
check('monte head names the assumption and output', (await page.locator('#monte-input').textContent()).length > 0 && (await page.locator('#monte-watch').textContent()).length > 0);
check('pipeline orientation hides once a workbook is loaded', await page.locator('#pipeline').isHidden());
await page.waitForFunction(() => /µs|ms/.test(document.getElementById('whatif-read')?.textContent ?? ''), null, { timeout: 30000 });
const read0 = await page.locator('#whatif-read').textContent();
check('what-if readout live with timing', /µs|ms/.test(read0));
const out0 = await page.locator('#whatif-read .ok').textContent(); // output value only — the µs suffix always changes
await page.locator('#whatif').fill('900');
await page.locator('#whatif').dispatchEvent('input');
await page.waitForFunction(
  (prev) => {
    const t = document.querySelector('#whatif-read .ok')?.textContent ?? '';
    return t !== '' && t !== prev;
  },
  out0,
  { timeout: 15000 },
);
const out1 = await page.locator('#whatif-read .ok').textContent();
check('slider changes the output', out1 !== out0 && /→/.test(await page.locator('#whatif-read').textContent()));
check('response curve painted', await page.locator('#curve').evaluate((cv) => {
  const ctx = cv.getContext('2d');
  return ctx.getImageData(0, 0, cv.width, cv.height).data.some((v) => v !== 0);
}));
// curve hover tooltip
await page.locator('#curve').hover({ position: { x: 400, y: 90 } });
check('curve tooltip on hover', await page.locator('#curve-tip').isVisible());

// --- Monte-Carlo ---
await page.locator('#run-monte').click();
await page.waitForFunction(() => !document.getElementById('monte-out')?.hidden, null, { timeout: 30000 });
const stats = await page.locator('#monte-stats').textContent();
check('monte stats line', /10,000 scenarios.*mean.*p95/.test(stats));
check('histogram painted', await page.locator('#hist').evaluate((cv) => {
  const ctx = cv.getContext('2d');
  return ctx.getImageData(0, 0, cv.width, cv.height).data.some((v) => v !== 0);
}));
await page.locator('#hist').hover({ position: { x: 420, y: 150 } });
check('histogram bin tooltip', await page.locator('#hist-tip').isVisible());
// prove the interactive worker survived the monte run: drag and expect a fresh answer
const preLive = await page.locator('#whatif-read .ok').textContent();
await page.locator('#whatif').fill('640');
await page.locator('#whatif').dispatchEvent('input');
await page.waitForFunction(
  (prev) => {
    const t = document.querySelector('#whatif-read .ok')?.textContent ?? '';
    return t !== '' && t !== prev;
  },
  preLive,
  { timeout: 15000 },
);
check('slider still live after monte', /→/.test(await page.locator('#whatif-read').textContent()));

// --- version diff (v2 = sample with one coefficient changed) ---
check('compare section appears', await page.locator('#compare').isVisible());
await page.locator('#file-b').setInputFiles(path.join(root, '../tests/cases/diff-b-sample.xlsx'));
await page.waitForFunction(() => !document.getElementById('diff-out')?.hidden, null, { timeout: 60000 });
const diffText = await page.locator('#diff-out').textContent();
check('diff reports divergence with witness', /diverges/.test(diffText) && /witness/.test(diffText) && /version A =/.test(diffText));

// --- v3: sticky nav + scrollspy ---
check('topnav appears after load', await page.locator('#topnav').isVisible());
await page.locator('#topnav a[data-spy="lab"]').click();
await page.waitForFunction(
  () => document.querySelector('#topnav a[data-spy="lab"]')?.classList.contains('active'),
  null,
  { timeout: 10000 },
);
check('nav click scrolls and scrollspy activates lab', true);
await page.evaluate(() => scrollTo(0, 0));
await page.waitForFunction(
  () => document.querySelector('#topnav a[data-spy="log"]')?.classList.contains('active'),
  null,
  { timeout: 10000 },
);
check('scrollspy returns to receipt at top', true);

// --- v3: caret diagnostics (rust-style, under the offending range) ---
check('caret line under finding formula', /\^/.test(await page.locator('.finding .caret').first().textContent()));

// --- v3: tornado sensitivity ---
await page.waitForFunction(() => !document.getElementById('tornado-wrap')?.hidden, null, { timeout: 30000 });
check('tornado painted', await page.locator('#tornado').evaluate((cv) => {
  const ctx = cv.getContext('2d');
  return ctx.getImageData(0, 0, cv.width, cv.height).data.some((v) => v !== 0);
}));
await page.locator('#tornado').hover({ position: { x: 300, y: 20 } });
check('tornado row tooltip', await page.locator('#tornado-tip').isVisible());

// --- v6: the drag must not grow the canvas or block the main thread ---
// (the canvas backing store used to double every paint at dpr>1, which
// blanked the tab mid-drag; and an event-driven repaint starved the
// compositor). Emulate dpr 2 for this check.
{
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 2, mobile: false });
  await page.locator('#whatif').scrollIntoViewIfNeeded();
  await page.waitForTimeout(200);
  // one repaint at the new ratio first — that resize is legitimate; what
  // must not happen is the store growing again on every later frame
  await page.locator('#whatif').fill('520');
  await page.locator('#whatif').dispatchEvent('input');
  await page.waitForTimeout(250);
  const before = await page.locator('#curve').evaluate((c) => `${c.width}x${c.height}`);
  await page.evaluate(() => {
    window.__drag = { long: 0, blocked: 0 };
    new PerformanceObserver((l) => { for (const e of l.getEntries()) { window.__drag.long++; window.__drag.blocked += e.duration; } })
      .observe({ entryTypes: ['longtask'] });
  });
  const box = await page.locator('#whatif').boundingBox();
  const cy = box.y + box.height / 2;
  await page.mouse.move(box.x + box.width * 0.5, cy);
  await page.mouse.down();
  for (let s = 0; s < 2; s++) for (let i = 0; i <= 30; i++) {
    const f = s % 2 === 0 ? i / 30 : 1 - i / 30;
    await page.mouse.move(box.x + box.width * (0.1 + 0.8 * f), cy);
  }
  await page.mouse.up();
  await page.waitForTimeout(300);
  const after = await page.locator('#curve').evaluate((c) => `${c.width}x${c.height}`);
  const dm = await page.evaluate(() => window.__drag);
  check('drag: canvas backing store stays fixed (no dpr runaway)', before === after);
  check(`drag: main thread never blocked (${dm.long} long tasks, ${Math.round(dm.blocked)}ms)`, dm.long === 0);
  check('drag: readout tracks the slider', /→/.test(await page.locator('#whatif-read').textContent()));
  await cdp.send('Emulation.clearDeviceMetricsOverride');
}

// --- v3: goal seek (bisection on the prepared cone, async in worker A) ---
await page.locator('#goal-target').fill('999999999999');
await page.locator('#goal-run').click();
await page.waitForFunction(() => /not reachable|found:|non-numeric/.test(document.getElementById('goal-read')?.textContent ?? ''), null, { timeout: 20000 });
check('goal seek reports unreachable targets honestly', /not reachable/.test(await page.locator('#goal-read').textContent()));
const preGoal = await page.locator('#whatif-read .ok').textContent();
await page.locator('#whatif').fill('800');
await page.locator('#whatif').dispatchEvent('input');
await page.waitForFunction(
  (prev) => {
    const t = document.querySelector('#whatif-read .ok')?.textContent ?? '';
    return t !== '' && t !== prev;
  },
  preGoal,
  { timeout: 15000 },
);
const goalTarget = (await page.locator('#whatif-read .ok').textContent()).replace(/,/g, '');
await page.locator('#whatif').fill('500');
await page.locator('#whatif').dispatchEvent('input');
await page.locator('#goal-target').fill(goalTarget);
await page.locator('#goal-run').click();
await page.waitForFunction(() => /found:|not reachable|non-numeric/.test(document.getElementById('goal-read')?.textContent ?? ''), null, { timeout: 20000 });
check('goal seek finds the input', /found:/.test(await page.locator('#goal-read').textContent()));

// --- v3: histogram/CDF view toggle (pixel-diff, not just non-blank) ---
const histPixels = await page.locator('#hist').evaluate((cv) => cv.toDataURL());
await page.locator('#view-cdf').click();
check('cdf view active', await page.locator('#view-cdf').evaluate((el) => el.classList.contains('active')));
check('cdf repaints the canvas', (await page.locator('#hist').evaluate((cv) => cv.toDataURL())) !== histPixels);
await page.locator('#view-hist').click();
check('toggle back restores the histogram', (await page.locator('#hist').evaluate((cv) => cv.toDataURL())) === histPixels);

// --- v3: command palette + export report download (content verified) ---
await page.locator('.finding [data-act="sup"]').first().click(); // 1 marked intentional for the export
await page.keyboard.press('Control+k');
check('ctrl-k opens the palette', await page.locator('#palette-ov').isVisible());
await page.locator('#palette-q').fill('zzzz-no-such-command');
check('palette shows the zero-match state', /no matching command/.test(await page.locator('#palette-list').textContent()));
await page.locator('#palette-q').fill('export');
const dlPromise = page.waitForEvent('download');
await page.keyboard.press('Enter');
const download = await dlPromise;
check('export downloads a markdown report', download.suggestedFilename().endsWith('-xlc-report.md'));
check('palette closes after running a command', await page.locator('#palette-ov').isHidden());
const report = fs.readFileSync(await download.path(), 'utf8');
check('report carries the receipt line', /29\/29 verifiable formula cells re-derived bit-exact \(100\.00%\)/.test(report));
check('report lists both findings with proofs', (report.match(/### (\[intentional\] )?warning\[/g) ?? []).length === 2 && /proof: /.test(report));
check('report marks the suppressed finding intentional', (report.match(/\[intentional\]/g) ?? []).length === 1);
check('report includes scenario stats and sensitivity table', /## scenario lab/.test(report) && /## sensitivity/.test(report));
await page.locator('.finding.suppressed [data-act="sup"]').click(); // restore

// --- v3: help overlay ---
await page.keyboard.press('?');
check('? opens keyboard help', await page.locator('#help-ov').isVisible());
await page.keyboard.press('Escape');
check('esc closes help', await page.locator('#help-ov').isHidden());

check('no console/page errors', errors.length === 0);
if (errors.length) console.log('errors:', errors.slice(0, 5));

if (coop) {
  const resp = await page.request.get(url);
  const h = resp.headers();
  const present =
    h['cross-origin-opener-policy'] === 'same-origin' &&
    h['cross-origin-embedder-policy'] === 'require-corp';
  const fs = await import('node:fs');
  fs.writeFileSync(
    path.join(root, '../docs/benchmarks/browser-coop.json'),
    JSON.stringify(
      { coop_coep_headers_present: present, checks_failed: failures, checks_run: checks },
      null,
      1,
    ),
  );
  console.log(`coop headers present: ${present}`);
}
await browser.close();
await server.close();
process.exit(failures === 0 ? 0 : 1);
