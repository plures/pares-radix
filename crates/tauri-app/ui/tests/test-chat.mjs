import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5195 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const errors = [];
page.on('console', m => { if(m.type()==='error') errors.push(m.text()); });
page.on('pageerror', e => errors.push('PAGE: '+e.message));
await page.goto('http://localhost:5195');
await page.waitForTimeout(3000);

const buttons = await page.locator('nav button').all();
console.log('Activity bar buttons:', buttons.length);
if (buttons.length > 0) {
  await buttons[0].click();
  await page.waitForTimeout(1000);
  const mainText = await page.locator('main').innerText();
  console.log('Main content after click:', mainText.slice(0, 300));
}
if (errors.length) console.log('Errors:', errors.join('\n'));
await browser.close();
await server.close();
