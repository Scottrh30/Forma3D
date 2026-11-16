// Opens the web build in Chromium, clicks through the interface and saves
// screenshots plus the browser console, so it can be checked without a
// Windows machine.
import { chromium } from 'playwright';
import fs from 'fs';

const url = process.argv[2] || 'http://localhost:8080/';
const out = process.argv[3] || 'preview';
const webgpu = process.argv[4] === 'webgpu';
fs.mkdirSync(out, { recursive: true });
const log = [];
const args = webgpu
  ? ['--enable-unsafe-webgpu', '--enable-features=Vulkan,WebGPU', '--use-angle=swiftshader', '--use-webgpu-adapter=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']
  : ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl', '--disable-features=WebGPU'];
const browser = await chromium.launch({ args });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
page.on('console', m => log.push(`[${m.type()}] ${m.text()}`));
page.on('pageerror', e => log.push(`[pageerror] ${e.message}`));
const wait = ms => page.waitForTimeout(ms);
const shot = async name => { await wait(500); await page.screenshot({ path: `${out}/${name}.png` }); log.push(`--- ${name}`); };
const commander = async text => {
  await page.keyboard.press('Shift+C');
  await wait(300);
  await page.keyboard.type(text);
  await wait(200);
  await page.keyboard.press('Enter');
  await wait(500);
};
const tab = async name => {
  const t = page.locator('.tab', { hasText: name }).first();
  if (await t.count()) { await t.click(); await wait(400); } else { log.push(`(no tab ${name})`); }
};
const menu = async name => {
  await page.locator('.menu-btn', { hasText: name }).first().dispatchEvent('mousedown');
  await wait(400);
};

await page.goto(webgpu ? url + '?webgpu' : url);
await wait(6000);
await shot('01-start');
if (webgpu) { await browser.close(); fs.writeFileSync(`${out}/console.txt`, log.join('\n')); process.exit(0); }

const stage = await page.$('.stage .hit');
const box = await stage.boundingBox();
const cx = box.x + box.width / 2, cy = box.y + box.height / 2;

// Select the cube in the Object Manager, orbit, open the Create menu.
const cube = page.locator('.tree .row', { hasText: 'Cube' }).first();
if (await cube.count()) { await cube.click(); }
await wait(400);
await page.mouse.move(box.x + box.width * 0.7, cy);
await page.mouse.down({ button: 'right' });
await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.45, { steps: 10 });
await page.mouse.up({ button: 'right' });
await wait(300);
await menu('Create');
await shot('02-create-menu');
await page.keyboard.press('Escape');

// Four views.
await commander('4 views');
await shot('03-four-views');
await commander('perspective view');

// Component modes: make the cube editable, Polygons mode, select all, extrude.
if (await cube.count()) { await cube.click(); }
await wait(300);
await page.keyboard.press('c');
await wait(400);
await page.keyboard.press('4');
await wait(300);
await page.keyboard.press('Control+a');
await shot('04-polygons-selected');
await page.keyboard.press('d');
await wait(300);
await page.mouse.move(cx, cy);
await page.mouse.down();
await page.mouse.move(cx + 40, cy, { steps: 5 });
await page.mouse.move(cx + 90, cy, { steps: 5 });
await shot('05-extrude-drag');
await page.mouse.up();
await wait(300);
// Points mode and a box selection.
await page.keyboard.press('Escape');
await page.keyboard.press('9');
await page.keyboard.press('2');
await wait(300);
await page.mouse.move(cx - 160, cy - 140);
await page.mouse.down();
await page.mouse.move(cx + 10, cy + 20, { steps: 8 });
await shot('06-points-box');
await page.mouse.up();
await wait(300);
await shot('07-points-selected');
await page.keyboard.press('e');
await tab('Structure');
await shot('08-structure');

// Edges mode with a loop selection, then the Mesh menu.
await page.keyboard.press('3');
await wait(300);
await page.mouse.dblclick(cx + 60, cy + 30);
await wait(300);
await menu('Mesh');
await shot('09-edges-mesh-menu');
await page.keyboard.press('Escape');

// Back to Model mode; the Spline Pen.
await page.keyboard.press('1');
await commander('spline pen');
for (const [dx, dy] of [[-200, 80], [-80, -60], [60, 70], [180, -40]]) {
  await page.mouse.click(cx + dx, cy + dy);
  await wait(150);
}
await page.mouse.move(cx + 220, cy + 60);
await shot('10-spline-pen');
await page.keyboard.press('Enter');
await wait(300);

// Snapping menu, layers and takes.
await menu('Modes');
const snapSub = page.locator('.dropdown .sub', { hasText: 'Snapping' }).first();
if (await snapSub.count()) { await snapSub.hover(); }
await shot('11-modes-snapping');
await page.keyboard.press('Escape');
await tab('Layers');
await page.locator('.bar .menu-btn', { hasText: 'Layer' }).first().dispatchEvent('mousedown');
await wait(300);
const newLayer = page.locator('.dropdown .mi', { hasText: 'Add to New Layer' }).first();
if (await newLayer.count()) { await newLayer.click(); }
await wait(300);
await tab('Takes');
await commander('new take');
await shot('12-layers-takes');

// Viewport options and right-click menu.
await commander('shadows');
await page.mouse.click(cx + 100, cy + 100, { button: 'right' });
await shot('13-context-menu');
await page.keyboard.press('Escape');
await page.mouse.click(10, 10);

// Workspaces.
await page.locator('.ws span', { hasText: 'UV Edit' }).first().click();
await shot('14-uv-workspace');
await page.locator('.ws span', { hasText: 'Sculpt' }).first().click();
await shot('15-sculpt-workspace');
await page.locator('.ws span', { hasText: 'Model' }).first().click();
await wait(300);

// Customize Commands and Preferences.
await commander('customize commands');
await shot('16-customize');
await page.keyboard.press('Escape');
await commander('preferences');
await shot('17-preferences');
await page.keyboard.press('Escape');

// Several projects: a new one, then the Window menu.
await page.keyboard.press('Control+n');
await wait(400);
await menu('Window');
await shot('18-window-projects');
await page.keyboard.press('Escape');

await tab('Console');
await shot('19-console');

fs.writeFileSync(`${out}/console.txt`, log.join('\n'));
await browser.close();
