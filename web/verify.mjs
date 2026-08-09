// Real-browser verification of the three-act sequence (Phase 4 Definition
// of Done). Serves the production build, drives headless Chromium, drops
// two workbooks, and exercises suppression persistence.
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
const check = (name, cond) => {
  console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}`);
  if (!cond) failures++;
};

await page.goto(url);
check('page title', (await page.title()).includes('xlc'));
check('drop zone visible', await page.locator('#drop').isVisible());

// Act sequence on the clean fixture.
await page.setInputFiles('#file', path.join(root, '../tests/cases/basic-sum.xlsx'));
await page.waitForFunction(() => document.getElementById('act3')?.textContent?.length > 0, null, { timeout: 20000 });
const act1a = await page.locator('#act1').textContent();
const act2a = await page.locator('#act2').textContent();
const act3a = await page.locator('#act3').textContent();
check('act1: compiled N formulas', /compiled 2 formulas across 1 sheet/.test(act1a));
check('act2: receipt 100%', /2\/2 verifiable cells.*\(100\.00%\)/.test(act2a));
check('act2: rendered green', await page.locator('#act2 .ok').count() === 1);
check('act3: zero defects', /0 defects found/.test(act3a));

// The planted slipped reference.
await page.setInputFiles('#file', path.join(root, '../tests/cases/slipped-ref.xlsx'));
await page.waitForFunction(() => /defect/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 20000 });
const act3b = await page.locator('#act3').textContent();
check('act3: one defect found', /1 defect found/.test(act3b));
const proof = await page.locator('.finding .proof').first().textContent();
check('finding carries proof', /7 of 8 cells share the copied formula pattern/.test(proof));
check('finding names the cell', (await page.locator('.finding .loc').first().textContent()).includes('Model!B4'));

// Suppression: click [intentional], count drops, persists across re-drop.
await page.locator('.finding button').first().click();
check('suppression flips count', /0 defects found/.test(await page.locator('#act3').textContent()));
check('suppressed style applied', await page.locator('.finding.suppressed').count() === 1);
await page.setInputFiles('#file', path.join(root, '../tests/cases/slipped-ref.xlsx'));
await page.waitForFunction(() => /marked intentional/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 20000 });
check('suppression persists across re-analysis', /0 defects found.*1 marked intentional/.test(await page.locator('#act3').textContent()));

check('no console/page errors', errors.length === 0);
if (errors.length) console.log('errors:', errors);

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
      { coop_coep_headers_present: present, checks_failed: failures, checks_run: 13 },
      null,
      1,
    ),
  );
  console.log(`coop headers present: ${present}`);
}
await browser.close();
await server.close();
process.exit(failures === 0 ? 0 : 1);
