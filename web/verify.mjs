// Real-browser verification of the v2 surface: three acts, findings
// triage (chips, copy, suppression, keyboard), the scenario lab (slider,
// response curve, Monte-Carlo histogram), and version diff.
import { chromium } from 'playwright';
import { preview } from 'vite';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

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
await page.locator('.chip[data-f="all"]').click();

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

// --- scenario lab ---
check('lab appears', await page.locator('#lab').isVisible());
check('input candidates listed', (await page.locator('#input-sel option').count()) > 3);
check('cone info shows schedule build', /cone: .* schedule built/.test(await page.locator('#cone-info').textContent()));
const read0 = await page.locator('#whatif-read').textContent();
check('what-if readout live with timing', /µs|ms/.test(read0));
await page.locator('#whatif').fill('900');
await page.locator('#whatif').dispatchEvent('input');
const read1 = await page.locator('#whatif-read').textContent();
check('slider changes the output', read1 !== read0 && /→/.test(read1));
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
const readAfter = await page.locator('#whatif-read').textContent();
check('slider still live after monte', /→/.test(readAfter));

// --- version diff (v2 = sample with one coefficient changed) ---
check('compare section appears', await page.locator('#compare').isVisible());
await page.locator('#file-b').setInputFiles(path.join(root, '../tests/cases/diff-b-sample.xlsx'));
await page.waitForFunction(() => !document.getElementById('diff-out')?.hidden, null, { timeout: 60000 });
const diffText = await page.locator('#diff-out').textContent();
check('diff reports divergence with witness', /diverges/.test(diffText) && /witness/.test(diffText) && /version A =/.test(diffText));

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
