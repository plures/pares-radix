import { chromium } from 'playwright';
import { createServer } from 'vite';
const server = await createServer({ root: '.', server: { port: 5191 } });
await server.listen();
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
await page.goto('http://localhost:5191');
await page.waitForTimeout(3000);

// Inspect the plugin registry from the browser
const plugins = await page.evaluate(() => {
  // Access Svelte stores is tricky — let's check the DOM instead
  const buttons = document.querySelectorAll('nav button');
  const info = [];
  buttons.forEach(b => info.push({ title: b.getAttribute('title'), text: b.textContent.trim() }));
  return info;
});
console.log('Buttons:', JSON.stringify(plugins));

// Check what's in each pane after clicking
const buttons = await page.locator('nav button').all();
for (let i = 0; i < buttons.length; i++) {
  await buttons[i].click();
  await page.waitForTimeout(500);
  // Check what component class is rendered in main
  const mainChildren = await page.evaluate(() => {
    const main = document.querySelector('main');
    if (!main) return 'no main';
    const section = main.querySelector('section');
    if (!section) return 'no section';
    // Get the first child element's class or tag
    const firstChild = section.children[0];
    if (!firstChild) return 'empty section';
    return firstChild.className + ' | ' + firstChild.tagName + ' | ' + (firstChild.textContent || '').slice(0, 50);
  });
  console.log(`After click ${i}: ${mainChildren}`);
}

await browser.close();
await server.close();
