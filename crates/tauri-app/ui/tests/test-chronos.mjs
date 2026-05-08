import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5189 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const chronosEntries = [];
page.on('console', m => {
  if (m.text().includes('[chronos]')) chronosEntries.push(m.text());
});
await page.goto('http://localhost:5189');
await page.waitForTimeout(3000);

// Click through views
const buttons = await page.locator('nav button').all();
for (let i = 0; i < Math.min(3, buttons.length); i++) {
  await buttons[i].click();
  await page.waitForTimeout(500);
}

console.log('=== CHRONOS LOG ===');
chronosEntries.forEach(e => console.log(e));
console.log(`Total entries: ${chronosEntries.length}`);

await browser.close();
await server.close();
