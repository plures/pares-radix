import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5192 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const logs = [];
page.on('console', m => logs.push(m.type() + ': ' + m.text()));
page.on('pageerror', e => logs.push('PAGEERROR: ' + e.message));
await page.goto('http://localhost:5192');
await page.waitForTimeout(3000);

const buttons = await page.locator('nav button').all();
console.log('Buttons:', buttons.length);

for (let i = 0; i < buttons.length; i++) {
  const title = await buttons[i].getAttribute('title');
  await buttons[i].click();
  await page.waitForTimeout(1500);
  const mainHtml = await page.locator('main').innerHTML();
  const hasWelcome = mainHtml.includes('welcome');
  const hasChat = mainHtml.includes('conversation') || mainHtml.includes('chat');
  const mainText = await page.locator('main').innerText();
  const firstLine = mainText.split('\n').filter(l => l.trim())[0] || '(empty)';
  console.log(`View ${i} (${title}): ${firstLine} | welcome=${hasWelcome} chat=${hasChat}`);
  
  // Take screenshot
  await page.screenshot({ path: `/tmp/radix-screenshots/debug-view-${i}.png` });
}

// Dump errors
const errors = logs.filter(l => l.startsWith('error') || l.startsWith('PAGEERROR'));
if (errors.length) console.log('ERRORS:', errors.join('\n'));

await browser.close();
await server.close();
