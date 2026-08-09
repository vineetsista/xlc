// Honest drag stress: REAL pointer drag (no artificial pacing), HiDPI,
// crash detection, FPS, long tasks, and engine-op timing.
// Usage: node stress-drag.mjs <xlsx> [--dpr 2]
import { chromium } from 'playwright';
import { preview } from 'vite';
const file = process.argv[2];
const dpr = Number((process.argv.find((a) => a.startsWith('--dpr=')) || '--dpr=2').split('=')[1]);
const server = await preview({ root: process.cwd(), preview: { port: 4182, strictPort: true } });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: dpr });
let crashed = false;
page.on('crash', () => (crashed = true));
const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
await page.goto('http://localhost:4182/');
const tLoad = Date.now();
await page.locator('#file').setInputFiles(file);
await page.waitForFunction(() => /defect|error/.test(document.getElementById('act3')?.textContent ?? '') || /error/.test(document.getElementById('act1')?.textContent ?? ''), null, { timeout: 180000 });
const auditMs = Date.now() - tLoad;
const formulas = (await page.locator('#act1').textContent()).trim();
// how long until the lab is actually usable?
const tPrep = Date.now();
let prepMs = -1;
try {
  await page.waitForFunction(() => /schedule built/.test(document.getElementById('cone-info')?.textContent ?? ''), null, { timeout: 120000 });
  prepMs = Date.now() - tPrep;
} catch { prepMs = -1; }
const cone = (await page.locator('#cone-info').textContent()).trim();

// instrument frames + long tasks, then perform a REAL drag
await page.evaluate(() => {
  window.__m = { frames: 0, long: 0, blocked: 0, reads: 0 };
  const tick = () => { window.__m.frames++; requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  new PerformanceObserver((l) => { for (const e of l.getEntries()) { window.__m.long++; window.__m.blocked += e.duration; } })
    .observe({ entryTypes: ['longtask'] });
  const r = document.getElementById('whatif-read');
  if (r) new MutationObserver(() => window.__m.reads++).observe(r, { childList: true, subtree: true, characterData: true });
});
await page.locator('#whatif').scrollIntoViewIfNeeded();
await page.waitForTimeout(200);
const box = await page.locator('#whatif').boundingBox();
const y = box.y + box.height / 2;
const t0 = Date.now();
await page.mouse.move(box.x + box.width * 0.5, y);
await page.mouse.down();
// 6 fast sweeps across the track, no waits — the real "drag too fast" case
for (let s = 0; s < 6; s++) {
  for (let i = 0; i <= 40; i++) {
    const frac = s % 2 === 0 ? i / 40 : 1 - i / 40;
    await page.mouse.move(box.x + box.width * (0.1 + 0.8 * frac), y);
  }
}
await page.mouse.up();
const dragMs = Date.now() - t0;
await page.waitForTimeout(500);
const m = crashed ? null : await page.evaluate(() => window.__m);
console.log(JSON.stringify({
  file: file.split('/').pop(), dpr, crashed, formulas, auditMs, prepMs, cone,
  dragMs, fps: m ? +(m.frames / (dragMs / 1000)).toFixed(1) : null,
  longTasks: m?.long, blockedMs: m ? Math.round(m.blocked) : null, readouts: m?.reads,
  errors: errors.slice(0, 3),
}, null, 1));
await browser.close(); await server.close();
