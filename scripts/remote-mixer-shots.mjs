#!/usr/bin/env node
// WebKit screenshots of the remote mixer page at iPhone 17 Pro Max and iPad sizes, both finishes.
// Start the preview first (node scripts/remote-mixer-preview.mjs), then:
//   PLAYWRIGHT=/path/to/node_modules/playwright/index.mjs node scripts/remote-mixer-shots.mjs <out-dir> [filter]
// Playwright is not a dependency of this repo; point PLAYWRIGHT at any install with WebKit.
const { webkit } = await import(process.env.PLAYWRIGHT ?? 'playwright');
const out = process.argv[2];
const SCALE = Number(process.env.SCALE ?? 1);
const base = process.env.BASE ?? 'http://127.0.0.1:4476';
const devices = {
  'phone-portrait': { viewport: { width: 440, height: 956 }, deviceScaleFactor: SCALE, isMobile: true, hasTouch: true },
  'phone-landscape': { viewport: { width: 956, height: 440 }, deviceScaleFactor: SCALE, isMobile: true, hasTouch: true },
  'tablet-portrait': { viewport: { width: 1032, height: 1376 }, deviceScaleFactor: SCALE, isMobile: true, hasTouch: true },
  'tablet-landscape': { viewport: { width: 1376, height: 1032 }, deviceScaleFactor: SCALE, isMobile: true, hasTouch: true },
};
const plan = (process.argv[3] ?? 'all');
const shots = [];
for (const finish of ['graphite', 'bone']) {
  for (const device of Object.keys(devices)) shots.push({ role: 'monitor2', device, finish });
  shots.push({ role: 'desk', device: 'tablet-landscape', finish });
  shots.push({ role: 'desk', device: 'tablet-portrait', finish });
}
shots.push({ role: 'monitor2', device: 'phone-portrait', finish: 'graphite', fine: true });
shots.push({ role: 'none', device: 'phone-portrait', finish: 'graphite' });
const browser = await webkit.launch();
const errors = [];
for (const shot of shots) {
  if (plan !== 'all' && !`${shot.role}-${shot.device}-${shot.finish}`.includes(plan)) continue;
  const context = await browser.newContext({ ...devices[shot.device] });
  await context.addInitScript(({ finish }) => {
    localStorage.setItem('tiwaton_remote_finish', finish);
    localStorage.setItem('tiwaton_remote_pins_monitor2', JSON.stringify(['Worship lead']));
  }, shot);
  const page = await context.newPage();
  page.on('pageerror', (e) => errors.push(`${shot.device}: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`${shot.device}: ${m.text()}`); });
  await page.goto(`${base}/__as/${shot.role}`);
  await page.waitForTimeout(2200);
  if (shot.fine) {
    await page.tap('#readout');
    await page.waitForTimeout(300);
  }
  const name = `${shot.role === 'monitor2' ? 'mon' : shot.role}-${shot.device}-${shot.finish}${shot.fine ? '-fine' : ''}.png`;
  await page.screenshot({ path: `${out}/${name}` });
  await context.close();
  console.log(name);
}
await browser.close();
console.log(errors.length ? errors.join('\n') : 'no page errors');
