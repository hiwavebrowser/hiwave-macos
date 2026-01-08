/**
 * capture_chrome.mjs - Capture screenshots using Playwright (Chromium)
 */

import { chromium } from 'playwright';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { existsSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Capture a screenshot of an HTML file using Chromium
 * 
 * @param {string} htmlPath - Path to HTML file
 * @param {string} outputPath - Path to save PNG screenshot
 * @param {number} width - Viewport width
 * @param {number} height - Viewport height
 * @returns {Promise<void>}
 */
export async function captureChrome(htmlPath, outputPath, width, height) {
  // Resolve to absolute path
  const absolutePath = resolve(htmlPath);
  
  if (!existsSync(absolutePath)) {
    throw new Error(`HTML file not found: ${absolutePath}`);
  }
  
  const browser = await chromium.launch({
    headless: true,
  });
  
  try {
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: 1,  // Ensure 1:1 pixel ratio for comparison
    });
    
    const page = await context.newPage();
    
    // Load the HTML file
    const fileUrl = `file://${absolutePath}`;
    await page.goto(fileUrl, { waitUntil: 'networkidle' });
    
    // Wait a bit for any CSS transitions/animations to settle
    await page.waitForTimeout(500);
    
    // Capture screenshot
    await page.screenshot({
      path: outputPath,
      type: 'png',
      fullPage: false,  // Only capture viewport
    });
    
    await context.close();
  } finally {
    await browser.close();
  }
}

/**
 * Capture screenshots for multiple cases
 * 
 * @param {Array<{id: string, htmlPath: string, width: number, height: number}>} cases
 * @param {string} outputDir - Directory to save screenshots
 * @returns {Promise<Object>} Results keyed by case ID
 */
export async function captureMultiple(cases, outputDir) {
  const browser = await chromium.launch({ headless: true });
  const results = {};
  
  try {
    for (const caseInfo of cases) {
      const { id, htmlPath, width, height } = caseInfo;
      const outputPath = `${outputDir}/${id}.png`;
      
      try {
        const context = await browser.newContext({
          viewport: { width, height },
          deviceScaleFactor: 1,
        });
        
        const page = await context.newPage();
        const fileUrl = `file://${resolve(htmlPath)}`;
        
        await page.goto(fileUrl, { waitUntil: 'networkidle' });
        await page.waitForTimeout(500);
        await page.screenshot({ path: outputPath, type: 'png', fullPage: false });
        
        await context.close();
        
        results[id] = { success: true, path: outputPath };
      } catch (err) {
        results[id] = { success: false, error: err.message };
      }
    }
  } finally {
    await browser.close();
  }
  
  return results;
}

