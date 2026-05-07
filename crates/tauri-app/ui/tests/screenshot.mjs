// tests/screenshot.mjs
import { chromium } from 'playwright';
import { createServer } from 'vite';
import { fileURLToPath } from 'url';
import { dirname, resolve } from 'path';
import { mkdirSync, writeFileSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');

async function main() {
  const server = await createServer({ root, server: { port: 5199 } });
  await server.listen();
  console.log('Vite dev server started on port 5199');

  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });

  await page.goto('http://localhost:5199');
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(2000);

  const outputDir = process.env.SCREENSHOT_DIR || '/tmp/radix-screenshots';
  mkdirSync(outputDir, { recursive: true });

  await page.screenshot({ path: `${outputDir}/01-initial.png`, fullPage: true });
  console.log(`Screenshot: ${outputDir}/01-initial.png`);

  const a11y = await page.locator('body').ariaSnapshot();
  writeFileSync(`${outputDir}/accessibility-tree.txt`, a11y);
  console.log(`A11y tree: ${outputDir}/accessibility-tree.txt`);

  const visibleText = await page.evaluate(() => document.body.innerText);
  writeFileSync(`${outputDir}/visible-text.txt`, visibleText);
  console.log(`Visible text: ${outputDir}/visible-text.txt`);

  const layout = await page.evaluate(() => {
    const elements = {};
    document.querySelectorAll('[class*="activity"], [class*="sidebar"], [class*="editor"], [class*="status"], [class*="shell"]').forEach(el => {
      const rect = el.getBoundingClientRect();
      elements[el.className.slice(0, 40)] = { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
    });
    return elements;
  });
  writeFileSync(`${outputDir}/layout-boxes.json`, JSON.stringify(layout, null, 2));
  console.log(`Layout: ${outputDir}/layout-boxes.json`);

  const activities = await page.$$('[class*="activity"] button, nav button');
  for (let i = 0; i < Math.min(activities.length, 6); i++) {
    await activities[i].click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${outputDir}/view-${i}.png` });
    console.log(`View ${i}: ${outputDir}/view-${i}.png`);
  }

  await browser.close();
  await server.close();
  console.log('Done. Screenshots at:', outputDir);
}

main().catch(e => { console.error(e); process.exit(1); });
