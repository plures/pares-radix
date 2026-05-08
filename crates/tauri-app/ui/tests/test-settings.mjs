import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5186 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto('http://localhost:5186');
await page.waitForTimeout(3000);
// Click Settings (5th button, index 4)
const buttons = await page.locator('nav button').all();
console.log('Clicking:', await buttons[4].getAttribute('title'));
await buttons[4].click();
await page.waitForTimeout(3000); // 3 second wait
const html = await page.locator('main').innerHTML();
console.log('HTML length:', html.length);
console.log('Contains "settings":', html.toLowerCase().includes('settings'));
console.log('Contains "provider":', html.toLowerCase().includes('provider'));
console.log('Contains "config-browser":', html.toLowerCase().includes('config-browser'));
const text = await page.locator('main').innerText();
console.log('Text (first 200):', text.slice(0,200));
await browser.close();
await server.close();
