import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5194 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on('console', m => console.log('CONSOLE:', m.type(), m.text()));
await page.goto('http://localhost:5194');
await page.waitForTimeout(3000);

// Check what plugins are registered
const pluginCount = await page.evaluate(() => {
  // Try to access the store - this won't work directly but let's see console
  return document.querySelectorAll('nav button').length;
});
console.log('Nav buttons:', pluginCount);

// Click the first one
const firstBtn = page.locator('nav button').first();
const btnText = await firstBtn.textContent();
console.log('Clicking button:', btnText);
await firstBtn.click();
await page.waitForTimeout(2000);

const mainText = await page.locator('main').innerText();
console.log('Main after click (first 200):', mainText.slice(0, 200));

await browser.close();
await server.close();
