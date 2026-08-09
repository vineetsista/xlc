// Drag-storm latency measurement: 120 slider input events at ~16ms pacing.
// Reports wall time, readout repaints, and long main-thread tasks.
import { chromium } from 'playwright';
import { preview } from 'vite';
const file = process.argv[2];
const server = await preview({ root: process.cwd(), preview: { port: 4178, strictPort: true } });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
await page.goto('http://localhost:4178/');
await page.locator('#file').setInputFiles(file);
await page.waitForFunction(() => /defect|0 defects/.test(document.getElementById('act3')?.textContent ?? ''), null, { timeout: 120000 });
await page.waitForFunction(() => !document.getElementById('lab')?.hidden, null, { timeout: 30000 });
await page.waitForFunction(() => /µs|ms/.test(document.getElementById('whatif-read')?.textContent ?? ''), null, { timeout: 30000 });
const res = await page.evaluate(async () => {
  const slider = document.getElementById('whatif');
  const read = document.getElementById('whatif-read');
  let repaints = 0;
  new MutationObserver(() => repaints++).observe(read, { childList: true, subtree: true, characterData: true });
  let longTasks = 0, longestMs = 0, blockedMs = 0;
  const po = new PerformanceObserver((l) => {
    for (const e of l.getEntries()) { longTasks++; blockedMs += e.duration; longestMs = Math.max(longestMs, e.duration); }
  });
  po.observe({ entryTypes: ['longtask'] });
  const t0 = performance.now();
  for (let i = 0; i < 120; i++) {
    slider.value = String(400 + (i % 40) * 5);
    slider.dispatchEvent(new Event('input', { bubbles: true }));
    await new Promise((r) => setTimeout(r, 16));
  }
  await new Promise((r) => setTimeout(r, 300));
  const wall = performance.now() - t0;
  return { wall: Math.round(wall), repaints, longTasks, longestMs: Math.round(longestMs), blockedMs: Math.round(blockedMs), cone: document.getElementById('cone-info')?.textContent };
});
console.log(JSON.stringify(res));
await browser.close(); await server.close();
