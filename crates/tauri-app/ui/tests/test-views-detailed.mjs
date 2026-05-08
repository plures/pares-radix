import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5188 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto('http://localhost:5188');
await page.waitForTimeout(3000);

const buttons = await page.locator('nav button').all();
for (let i = 0; i < buttons.length; i++) {
  const title = await buttons[i].getAttribute('title');
  await buttons[i].click();
  await page.waitForTimeout(2000); // longer wait
  const mainText = await page.locator('main').innerText();
  const lines = mainText.split('\n').filter(l => l.trim());
  console.log(`\n=== ${title} (${lines.length} lines) ===`);
  lines.slice(0, 5).forEach(l => console.log('  ' + l));
  await page.screenshot({ path: `/home/kbristol/.openclaw/workspace/radix-view-${i}-${title.replace(/\s/g,'')}.png` });
}

await browser.close();
await server.close();
