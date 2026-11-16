// Opens the web build in Chromium, clicks through a few things and saves
// screenshots plus the browser console, so the interface can be checked
// without a Windows machine.
import { chromium } from 'playwright';
import fs from 'fs';

const url = process.argv[2] || 'http://localhost:8080/';
const out = process.argv[3] || 'preview';
fs.mkdirSync(out, { recursive: true });
const log = [];
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl'] });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
page.on('console', m => log.push(`[${m.type()}] ${m.text()}`));
page.on('pageerror', e => log.push(`[pageerror] ${e.message}`));
await page.goto(url);
await page.waitForTimeout(6000);
await page.screenshot({ path: `${out}/1-start.png` });

const stage = await page.$('.stage .hit');
const box = await stage.boundingBox();
// Click the middle of the view (selects what is there), then the cube in the Object Manager.
await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
await page.waitForTimeout(800);
const cube = page.locator('.tree .row', { hasText: 'Cube' }).first();
if (await cube.count()) { await cube.click(); }
await page.waitForTimeout(800);
await page.screenshot({ path: `${out}/2-selected.png` });

// Orbit with the right button, then open the Create menu.
await page.mouse.move(box.x + box.width * 0.7, box.y + box.height * 0.5);
await page.mouse.down({ button: 'right' });
await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.45, { steps: 10 });
await page.mouse.up({ button: 'right' });
await page.waitForTimeout(600);
await page.locator('.menu-btn', { hasText: 'Create' }).first().dispatchEvent('mousedown');
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/3-orbit-menu.png` });
await page.keyboard.press('Escape');

// Add a torus from the command search, open the Object tab.
await page.keyboard.press('Shift+C');
await page.waitForTimeout(400);
await page.keyboard.type('torus');
await page.keyboard.press('Enter');
await page.waitForTimeout(800);
const objTab = page.locator('.seg span', { hasText: 'Object' }).first();
if (await objTab.count()) { await objTab.click(); }
await page.waitForTimeout(600);
await page.locator('.tab', { hasText: 'Asset Browser' }).first().click();
await page.waitForTimeout(600);
await page.screenshot({ path: `${out}/4-torus-assets.png` });
await page.locator('.tab', { hasText: 'Console' }).first().click();
await page.waitForTimeout(400);
await page.screenshot({ path: `${out}/5-console.png` });

fs.writeFileSync(`${out}/console.txt`, log.join('\n'));
await browser.close();
