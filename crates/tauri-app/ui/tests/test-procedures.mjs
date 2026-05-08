import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5187 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto('http://localhost:5187');
await page.waitForTimeout(3000);
// Click Procedures (2nd button)
const buttons = await page.locator('nav button').all();
await buttons[1].click();
await page.waitForTimeout(2000);
const html = await page.locator('main').innerHTML();
console.log('HTML length:', html.length);
console.log('Contains "procedure":', html.toLowerCase().includes('procedure'));
console.log('Contains "dialog":', html.toLowerCase().includes('dialog'));
console.log('First 500 chars:', html.slice(0, 500));
await browser.close();
await server.close();
