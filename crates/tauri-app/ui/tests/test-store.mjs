import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5190 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on('console', m => { if (m.text().includes('[radix]')) console.log(m.text()); });
await page.goto('http://localhost:5190');
await page.waitForTimeout(3000);

// Inject logging into the click handler
await page.evaluate(() => {
  // Monkey-patch to see what's happening
  const orig = window.__radixDebug;
});

// Click each button and check what the store says
const buttons = await page.locator('nav button').all();
for (let i = 0; i < buttons.length; i++) {
  const title = await buttons[i].getAttribute('title');
  await buttons[i].click();
  await page.waitForTimeout(500);
  
  // Read the pane state from the DOM data attributes or evaluate
  const paneInfo = await page.evaluate(() => {
    const panes = document.querySelectorAll('.radix-pane, section');
    return Array.from(panes).map(p => ({
      classes: p.className,
      childClasses: p.children[0]?.className || 'none',
    }));
  });
  console.log(`Click ${i} (${title}): panes=${JSON.stringify(paneInfo)}`);
}

await browser.close();
await server.close();
