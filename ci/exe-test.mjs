// Drives the real Forma3D.exe on a Windows runner. The app's WebView2 is
// started with a remote-debugging port, so Playwright can click through the
// actual window (not a browser copy of it) and take screenshots.
import { chromium } from 'playwright';
import fs from 'fs';

const out = process.argv[2] || 'exe-test';
fs.mkdirSync(out, { recursive: true });
const log = [];
const note = s => { log.push(s); console.log(s); };

const browser = await chromium.connectOverCDP('http://127.0.0.1:9222');
let page;
for (let i = 0; i < 60 && !page; i++) {
  page = browser.contexts().flatMap(c => c.pages()).find(p => !p.url().startsWith('devtools'));
  if (!page) await new Promise(r => setTimeout(r, 1000));
}
if (!page) { note('FAIL: no app page'); process.exit(1); }
note(`page: ${page.url()}`);
page.on('console', m => log.push(`[${m.type()}] ${m.text()}`));
page.on('pageerror', e => log.push(`[pageerror] ${e.message}`));
const wait = ms => page.waitForTimeout(ms);
const shot = async name => { await wait(800); await page.screenshot({ path: `${out}/${name}.png` }); note(`--- ${name}`); };
const commander = async text => {
  await page.keyboard.press('Shift+C');
  await wait(300);
  await page.keyboard.type(text);
  await wait(250);
  await page.keyboard.press('Enter');
  await wait(600);
};
const status = async () => (await page.locator('.statusbar').first().innerText().catch(() => '')).replace(/\s+/g, ' ');

await wait(8000);
await shot('01-start');
const hud = await page.evaluate(() => document.body.innerText.match(/WebGPU|WebGL[^\n]*/)?.[0] ?? '(no graphics label)');
const gpu = await page.evaluate(async () => {
  if (!navigator.gpu) return 'navigator.gpu missing';
  const a = await navigator.gpu.requestAdapter();
  if (!a) return 'no WebGPU adapter';
  const i = a.info || {};
  return `WebGPU adapter: ${i.vendor || '?'} ${i.architecture || ''} ${i.device || ''} ${i.description || ''}`.trim();
});
note(`graphics in use: ${hud}`);
note(gpu);
note(`window size: ${JSON.stringify(await page.evaluate(() => [innerWidth, innerHeight, devicePixelRatio]))}`);

// Some shapes and three lights, with shadows from every light.
await commander('cube');
await commander('sphere');
await commander('torus');
await commander('area light');
await commander('spot light');
await commander('omni light');
await commander('frame all');
await commander('shadows');
await shot('02-shadows-every-light');
await commander('screen-space reflections');
await shot('03-ssr');
await commander('safe frames');
await shot('04-safe-frames');
await commander('3 views, large left');
await shot('05-three-views');
await commander('isometric');
await shot('06-isometric');
await commander('undo view');
await shot('07-undo-view');
await page.keyboard.press('Shift+V');
await wait(800);
const dialog = await page.locator('.viewcfg').count();
note(`Configure View dialog open after Shift+V: ${dialog > 0}`);
await shot('08-configure-view');
await page.keyboard.press('Escape');
await page.mouse.click(5, 300);
await commander('4 views');
await shot('09-four-views');
note(`status: ${await status()}`);

// Rough frame rate while orbiting the perspective view.
const fps = await page.evaluate(() => new Promise(res => {
  let n = 0; const t0 = performance.now();
  const tick = () => { n++; if (performance.now() - t0 < 3000) requestAnimationFrame(tick); else res(n / 3); };
  requestAnimationFrame(tick);
}));
note(`frames per second (idle, 3 s): ${fps.toFixed(1)}`);

fs.writeFileSync(`${out}/log.txt`, log.join('\n'));
await browser.close();
