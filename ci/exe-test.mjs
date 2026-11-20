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

// Reload once, so the console from start-up (graphics set-up) is captured too.
await page.reload();
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

// Three lights, with shadows from every light (framed on the shapes).
await commander('frame all');
await commander('area light');
await commander('spot light');
await commander('omni light');
await commander('deselect all');
await commander('shadows');
await shot('02-shadows-every-light');
// Each offscreen effect on its own, then off again.
for (const [fx, name] of [['post effects', '03a-post-effects'], ['ssao', '03b-ssao'], ['screen-space reflections', '03c-ssr']]) {
  await commander(fx);
  await shot(name);
  await commander(fx);
}
await commander('screen-space reflections');
await commander('safe frames');
await shot('04-safe-frames');
await commander('3 views, large left');
await shot('05-three-views');
await page.mouse.click(...(await (async () => { const b = await page.locator('.stage .hit').first().boundingBox(); return [b.x + b.width * 0.85, b.y + b.height * 0.2]; })()));
await commander('isometric');
await shot('06-isometric');
await commander('undo view');
await shot('07-undo-view');
await page.keyboard.press('Shift+V');
await wait(800);
let dialog = await page.locator('.viewcfg').count();
note(`Configure View dialog open after Shift+V: ${dialog > 0}`);
if (!dialog) {
  await commander('configure view');
  dialog = await page.locator('.viewcfg').count();
  note(`Configure View dialog open from the Commander: ${dialog > 0}`);
}
await shot('08-configure-view');
await page.mouse.click(5, 300);

// Render Settings, then a Physical render to the Picture Viewer.
await commander('edit render settings');
await wait(600);
note(`Render Settings sections: ${(await page.locator('.rs-tab').allInnerTexts()).join(', ')}`);
await shot('10-render-settings-standard');
const engine = page.locator('.rsettings select').first();
if (await engine.count()) {
  await engine.selectOption('Physical');
  await wait(600);
  await shot('11-render-settings-physical');
}
await page.mouse.click(5, 300);
await commander('render to picture viewer');
const t0 = Date.now();
await wait(15000);
await shot('12-physical-render');
note(`after ${(Date.now() - t0) / 1000}s: ${await status()}`);
await commander('stop rendering');
await commander('4 views');
await shot('09-four-views');
note(`status: ${await status()}`);

// Import an OBJ with its MTL (a red pyramid) through File > Import.
fs.writeFileSync(`${out}/pyramid.obj`, 'mtllib pyramid.mtl\no Pyramid\nv -100 0 -100\nv 100 0 -100\nv 100 0 100\nv -100 0 100\nv 0 150 0\nusemtl Brick\nf 1 2 3 4\nf 1 5 2\nf 2 5 3\nf 3 5 4\nf 4 5 1\n');
fs.writeFileSync(`${out}/pyramid.mtl`, 'newmtl Brick\nKd 0.8 0.2 0.1\n');
try {
  await page.mouse.click(5, 300);
  const chooser = page.waitForEvent('filechooser', { timeout: 10000 });
  await commander('import');
  await (await chooser).setFiles([`${out}/pyramid.obj`, `${out}/pyramid.mtl`]);
  await wait(1000);
  await shot('13-import-dialog');
  await page.locator('.iodlg .btn.primary').click();
  await wait(1500);
  note(`import: ${await status()}`);
  await commander('frame all');
  await shot('14-imported-pyramid');
  await commander('export');
  await wait(500);
  note(`export formats: ${(await page.locator('.iodlg select').first().locator('option').allInnerTexts()).join(', ')}`);
  await shot('15-export-dialog');
  await page.locator('.iodlg .btn').first().click();
} catch (e) {
  note(`FAIL import/export: ${e.message}`);
}

// Rough frame rate while orbiting the perspective view.
const fps = await page.evaluate(() => new Promise(res => {
  let n = 0; const t0 = performance.now();
  const tick = () => { n++; if (performance.now() - t0 < 3000) requestAnimationFrame(tick); else res(n / 3); };
  requestAnimationFrame(tick);
}));
note(`frames per second (idle, 3 s): ${fps.toFixed(1)}`);

fs.writeFileSync(`${out}/log.txt`, log.join('\n'));
await browser.close();
