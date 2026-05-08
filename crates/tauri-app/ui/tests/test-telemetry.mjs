import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5185 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });

// Collect ALL chronos entries
const chronos = [];
page.on('console', m => {
  if (m.text().includes('[chronos]')) chronos.push(m.text());
});

await page.goto('http://localhost:5185');
await page.waitForTimeout(3000);

// Simulate user session: switch views, interact
const buttons = await page.locator('nav button').all();
for (let i = 0; i < buttons.length; i++) {
  await buttons[i].click();
  await page.waitForTimeout(800);
}

// Go back to chat
await buttons[0].click();
await page.waitForTimeout(1000);

// Get in-memory log via window API
const logCount = await page.evaluate(() => {
  return window.__radixDiag ? window.__radixDiag(100).split('\n').length : 0;
});

console.log('\n=== CHRONOS TELEMETRY REPORT ===');
console.log(`Total entries captured: ${chronos.length}`);
console.log(`In-memory log entries: ${logCount}`);
console.log('\n--- Events by type ---');
const types = {};
chronos.forEach(e => {
  const match = e.match(/\[chronos\] (\w+)/);
  if (match) types[match[1]] = (types[match[1]] || 0) + 1;
});
Object.entries(types).sort((a, b) => b[1] - a[1]).forEach(([t, c]) => console.log(`  ${t}: ${c}`));

console.log('\n--- Full timeline ---');
chronos.forEach(e => console.log(e));

await browser.close();
await server.close();
