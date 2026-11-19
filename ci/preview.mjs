// Opens the web build in Chromium, clicks through the interface and saves
// screenshots plus the browser console, so it can be checked without a
// Windows machine.
import { chromium } from 'playwright';
import fs from 'fs';

const url = process.argv[2] || 'http://localhost:8080/';
const out = process.argv[3] || 'preview';
// The full run uses the default graphics (WebGPU); `webgl` is a short run
// that forces WebGL 2 to check the fallback and its grid.
const webgl = process.argv[4] === 'webgl';
fs.mkdirSync(out, { recursive: true });
const log = [];
// (--use-vulkan=swiftshader lets screenshots capture the WebGPU canvas.)
const args = webgl
  ? ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl', '--disable-features=WebGPU']
  : ['--enable-unsafe-webgpu', '--enable-features=Vulkan,WebGPU', '--use-vulkan=swiftshader', '--use-angle=swiftshader', '--use-webgpu-adapter=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];
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

await page.goto(webgl ? url + '?webgl' : url);
await wait(6000);
await shot('01-start');
if (webgl) {
  await commander('4 views');
  await shot('02-four-views');
  await browser.close();
  fs.writeFileSync(`${out}/console.txt`, log.join('\n'));
  process.exit(0);
}

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

// Rendering: a sky, Render View, Render Settings, the Picture Viewer,
// the Light Lister and looking through a camera.
await commander('physical sky');
await commander('render view');
await wait(5000);
await shot('19-render-view');
await commander('edit render settings');
await shot('20-render-settings');
await page.keyboard.press('Escape');
await commander('render to picture viewer');
await wait(8000);
await shot('21-picture-viewer');
await commander('stop rendering');
await commander('light lister');
await shot('22-light-lister');
await menu('Create');
const camSub = page.locator('.dropdown .sub', { hasText: 'Camera' }).first();
if (await camSub.count()) { await camSub.hover(); }
await shot('23-camera-menu');
await page.keyboard.press('Escape');
await page.mouse.click(10, 10);
await commander('target camera');
await commander('look through camera');
await page.mouse.move(cx, cy);
await page.mouse.down({ button: 'right' });
await page.mouse.move(cx - 60, cy + 20, { steps: 8 });
await page.mouse.up({ button: 'right' });
await shot('24-look-through');
await commander('editor camera');

// Dragging a tab to dock it: Takes onto the bottom-left group, then a
// panel onto the empty side zone along the viewport.
{
  const t = page.locator('.tab', { hasText: 'Takes' }).first();
  const target = page.locator('.bottom .panel .tabs').first();
  if (await t.count() && await target.count()) {
    const a = await t.boundingBox(), b = await target.boundingBox();
    await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
    await page.mouse.down();
    await page.mouse.move(b.x + b.width - 20, b.y + b.height / 2, { steps: 12 });
    await shot('25-dock-drag');
    await page.mouse.up();
    await wait(300);
  } else { log.push('(no Takes tab to drag)'); }
  const l = page.locator('.tab', { hasText: 'Layers' }).first();
  const st = await page.$('.center .stage');
  if (await l.count() && st) {
    const a = await l.boundingBox(), s2 = await st.boundingBox();
    await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
    await page.mouse.down();
    await page.mouse.move(s2.x + 60, s2.y + s2.height / 2, { steps: 12 });
    await shot('26-dock-zones');
    await page.mouse.up();
    await wait(400);
    await shot('27-docked-side');
  }
}

// The icon bar: open the Icon Library and drag an icon onto the bar.
await page.locator('.ib-add').first().click();
await wait(300);
{
  const icon = page.locator('.iconlib .il-grid .tb').nth(5);
  const bar = page.locator('.iconbar').first();
  if (await icon.count() && await bar.count()) {
    const a = await icon.boundingBox(), b = await bar.boundingBox();
    await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
    await page.mouse.down();
    await page.mouse.move(b.x + 120, b.y + b.height / 2, { steps: 10 });
    await shot('28-iconbar-drag');
    await page.mouse.up();
  }
}
await wait(300);
await page.keyboard.press('Escape');

// Save Selection to Asset Browser.
await commander('select all');
await commander('save selection to asset browser');
await tab('Asset Browser');
await shot('29-my-assets');

// The material editor.
await commander('new material');
await tab('Materials');
await shot('30-material-editor');

// Viewport: shadows from every light, area lights, reflections, layouts,
// projections, Safe Frames and Configure View.
await commander('area light');
await commander('spot light');
await commander('omni light');
await commander('frame all');
await commander('shadows');
await shot('32-shadows-every-light');
await commander('screen-space reflections');
await shot('33-ssr');
await commander('safe frames');
await shot('34-safe-frames');
await commander('3 views, large left');
await shot('35-three-views');
await page.mouse.click(box.x + box.width * 0.8, box.y + box.height * 0.25);
await commander('isometric');
await shot('36-isometric');
await page.mouse.move(cx, cy);
await page.mouse.down({ button: 'right' });
await page.mouse.move(cx + 80, cy - 30, { steps: 8 });
await page.mouse.up({ button: 'right' });
await commander('undo view');
await shot('37-undo-view');
await commander('configure view');
await shot('38-configure-view');
await page.mouse.click(8, 990);
await commander('1 view');

await tab('Console');
await shot('31-console');

fs.writeFileSync(`${out}/console.txt`, log.join('\n'));
await browser.close();
