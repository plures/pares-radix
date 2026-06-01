#!/usr/bin/env node
/**
 * AI Visual QA — Screenshot-based UI regression testing
 *
 * Takes screenshots of pares-radix pages and evaluates them
 * using a vision model for layout correctness, accessibility,
 * and visual regressions.
 *
 * Usage:
 *   node visual-qa.mjs [--baseline] [--compare] [--pages chat,canvas,settings]
 *
 * Modes:
 *   --baseline: Capture baseline screenshots (first run or after intentional changes)
 *   --compare:  Compare current screenshots against baseline using vision model
 *   --pages:    Comma-separated list of pages to test (default: all)
 *
 * Requirements:
 *   - Playwright (npx playwright install chromium)
 *   - Running pares-radix dev server (default: http://localhost:5173)
 *   - OPENAI_API_KEY or configured vision model for comparison mode
 */

import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';
import { execSync } from 'child_process';

const BASE_URL = process.env.RADIX_URL || 'http://localhost:5173';
const SCREENSHOT_DIR = join(import.meta.dirname, '../screenshots');
const BASELINE_DIR = join(SCREENSHOT_DIR, 'baseline');
const CURRENT_DIR = join(SCREENSHOT_DIR, 'current');
const RESULTS_DIR = join(SCREENSHOT_DIR, 'results');

// Pages to test with their expected visual characteristics
const PAGES = {
  chat: {
    path: '/chat',
    viewport: { width: 1280, height: 720 },
    expectations: [
      'Chat interface with message input area at bottom',
      'Left sidebar with navigation',
      'Clean typography, readable font sizes',
      'Dark or light theme applied consistently',
    ],
  },
  canvas: {
    path: '/canvas',
    viewport: { width: 1280, height: 720 },
    expectations: [
      'Canvas workspace area',
      'Component tree or hierarchy visible',
      'No overlapping elements or broken layouts',
    ],
  },
  settings: {
    path: '/settings',
    viewport: { width: 1280, height: 720 },
    expectations: [
      'Settings panel with form controls',
      'Proper spacing and alignment',
      'Labels associated with inputs',
    ],
  },
  design: {
    path: '/design',
    viewport: { width: 1280, height: 720 },
    expectations: [
      'Design system components displayed',
      'Consistent color palette',
      'Interactive components visible',
    ],
  },
};

function parseArgs() {
  const args = process.argv.slice(2);
  const mode = args.includes('--baseline') ? 'baseline' : 'compare';
  const pagesArg = args.find(a => a.startsWith('--pages='))?.split('=')[1];
  const pages = pagesArg ? pagesArg.split(',') : Object.keys(PAGES);
  const verbose = args.includes('--verbose');
  return { mode, pages, verbose };
}

async function captureScreenshots(pages, outputDir) {
  mkdirSync(outputDir, { recursive: true });

  const browser = await chromium.launch({ headless: true });
  const results = [];

  for (const pageName of pages) {
    const pageConfig = PAGES[pageName];
    if (!pageConfig) {
      console.warn(`Unknown page: ${pageName}, skipping`);
      continue;
    }

    const context = await browser.newContext({
      viewport: pageConfig.viewport,
      colorScheme: 'dark',
    });
    const page = await context.newPage();

    try {
      const url = `${BASE_URL}${pageConfig.path}`;
      console.log(`  📸 Capturing ${pageName} (${url})`);

      await page.goto(url, { waitUntil: 'networkidle', timeout: 15000 });
      // Wait a bit for any animations to settle
      await page.waitForTimeout(1000);

      const screenshotPath = join(outputDir, `${pageName}.png`);
      await page.screenshot({ path: screenshotPath, fullPage: false });

      results.push({
        page: pageName,
        path: screenshotPath,
        url,
        viewport: pageConfig.viewport,
        expectations: pageConfig.expectations,
        status: 'captured',
      });
    } catch (err) {
      results.push({
        page: pageName,
        status: 'error',
        error: err.message,
      });
      console.error(`  ❌ Failed to capture ${pageName}: ${err.message}`);
    }

    await context.close();
  }

  await browser.close();
  return results;
}

async function compareWithVisionModel(currentResults) {
  // Vision model comparison using OpenAI API
  const apiKey = process.env.OPENAI_API_KEY;
  if (!apiKey) {
    console.error('OPENAI_API_KEY required for --compare mode');
    process.exit(1);
  }

  const results = [];

  for (const result of currentResults) {
    if (result.status !== 'captured') continue;

    const baselinePath = join(BASELINE_DIR, `${result.page}.png`);
    if (!existsSync(baselinePath)) {
      results.push({
        ...result,
        comparison: 'no_baseline',
        message: 'No baseline screenshot found. Run with --baseline first.',
      });
      continue;
    }

    const currentImage = readFileSync(result.path).toString('base64');
    const baselineImage = readFileSync(baselinePath).toString('base64');

    console.log(`  🔍 Comparing ${result.page} against baseline...`);

    const prompt = `You are a UI quality assurance expert. Compare these two screenshots of a web application page.

The first image is the BASELINE (known good state).
The second image is the CURRENT version being tested.

Page: ${result.page}
Expected characteristics:
${result.expectations.map(e => `- ${e}`).join('\n')}

Analyze:
1. Are there any visual regressions (layout breaks, missing elements, misalignment)?
2. Is the visual hierarchy maintained?
3. Are colors and typography consistent?
4. Any accessibility concerns (contrast, readability)?

Respond with JSON:
{
  "pass": true/false,
  "score": 0-100 (visual quality score),
  "regressions": ["list of regressions found"],
  "notes": "brief summary"
}`;

    try {
      const response = await fetch('https://api.openai.com/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
          model: 'gpt-4o',
          messages: [
            {
              role: 'user',
              content: [
                { type: 'text', text: prompt },
                { type: 'image_url', image_url: { url: `data:image/png;base64,${baselineImage}` } },
                { type: 'image_url', image_url: { url: `data:image/png;base64,${currentImage}` } },
              ],
            },
          ],
          max_tokens: 500,
          response_format: { type: 'json_object' },
        }),
      });

      const data = await response.json();
      const analysis = JSON.parse(data.choices[0].message.content);

      results.push({
        ...result,
        comparison: analysis.pass ? 'pass' : 'fail',
        score: analysis.score,
        regressions: analysis.regressions || [],
        notes: analysis.notes,
      });

      const icon = analysis.pass ? '✅' : '❌';
      console.log(`  ${icon} ${result.page}: score=${analysis.score}/100 ${analysis.notes}`);
    } catch (err) {
      results.push({
        ...result,
        comparison: 'error',
        error: err.message,
      });
    }
  }

  return results;
}

async function main() {
  const { mode, pages, verbose } = parseArgs();

  console.log(`\n🎨 AI Visual QA — ${mode} mode`);
  console.log(`   Pages: ${pages.join(', ')}`);
  console.log(`   URL: ${BASE_URL}\n`);

  if (mode === 'baseline') {
    console.log('📷 Capturing baseline screenshots...\n');
    const results = await captureScreenshots(pages, BASELINE_DIR);
    const captured = results.filter(r => r.status === 'captured').length;
    console.log(`\n✅ Baseline captured: ${captured}/${pages.length} pages`);
    console.log(`   Saved to: ${BASELINE_DIR}`);

    // Save metadata
    writeFileSync(
      join(BASELINE_DIR, 'metadata.json'),
      JSON.stringify({ capturedAt: new Date().toISOString(), pages: results }, null, 2)
    );
  } else {
    console.log('📷 Capturing current screenshots...\n');
    const currentResults = await captureScreenshots(pages, CURRENT_DIR);

    console.log('\n🤖 Running AI comparison...\n');
    const comparisonResults = await compareWithVisionModel(currentResults);

    // Save results
    mkdirSync(RESULTS_DIR, { recursive: true });
    const report = {
      runAt: new Date().toISOString(),
      baseUrl: BASE_URL,
      results: comparisonResults,
      summary: {
        total: comparisonResults.length,
        passed: comparisonResults.filter(r => r.comparison === 'pass').length,
        failed: comparisonResults.filter(r => r.comparison === 'fail').length,
        errors: comparisonResults.filter(r => r.comparison === 'error' || r.comparison === 'no_baseline').length,
      },
    };

    writeFileSync(join(RESULTS_DIR, 'report.json'), JSON.stringify(report, null, 2));

    console.log('\n📊 Summary:');
    console.log(`   Pass: ${report.summary.passed}`);
    console.log(`   Fail: ${report.summary.failed}`);
    console.log(`   Error/No baseline: ${report.summary.errors}`);
    console.log(`   Report: ${join(RESULTS_DIR, 'report.json')}`);

    // Exit with failure if any regressions
    if (report.summary.failed > 0) {
      process.exit(1);
    }
  }
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
