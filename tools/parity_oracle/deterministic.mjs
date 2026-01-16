/**
 * deterministic.mjs - Shared deterministic settings for Playwright Chromium captures
 *
 * This module centralizes:
 * - Launch flags (color profile, font AA behavior, background throttling)
 * - Context settings (viewport, DPR)
 * - Init scripts (parity-freeze.js, optional parity-reset.css for micro-tests)
 * - Font loading for consistent text rendering
 */

import { readFileSync, existsSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, '../..');

const FREEZE_JS_PATH = join(REPO_ROOT, 'baselines', 'common', 'parity-freeze.js');
const RESET_CSS_PATH = join(REPO_ROOT, 'baselines', 'common', 'parity-reset.css');
const FONTS_DIR = join(REPO_ROOT, 'baselines', 'common', 'fonts');

// Read the reset CSS and fix font paths to use absolute file:// URLs
function getResetCssWithAbsoluteFontPaths() {
  if (!existsSync(RESET_CSS_PATH)) return '';
  let css = readFileSync(RESET_CSS_PATH, 'utf8');
  // Replace relative font URLs with absolute file:// URLs
  css = css.replace(/url\(['"]?\/baselines\/common\/fonts\//g, `url('file://${FONTS_DIR}/`);
  css = css.replace(/format\('truetype'\)\s*;/g, `format('truetype');`);
  return css;
}

const RESET_CSS = getResetCssWithAbsoluteFontPaths();

export function getDeterministicLaunchOptions() {
  // Note: some flags may be ignored depending on platform/Chromium build.
  // We keep them because they reduce variance in practice.
  const args = [
    '--force-color-profile=srgb',
    '--disable-gpu-vsync',
    '--disable-features=RendererCodeIntegrity',
    '--disable-background-timer-throttling',
    '--disable-backgrounding-occluded-windows',
    '--disable-renderer-backgrounding',
    '--disable-lcd-text',
    '--disable-font-subpixel-positioning',
    '--disable-accelerated-2d-canvas',
    '--use-gl=swiftshader',
  ];

  return {
    headless: true,
    args,
  };
}

export async function createDeterministicContext(browser, width, height, options = {}) {
  const { applyParityReset = false } = options;

  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: 1,
  });

  // Freeze time/animations as early as possible.
  if (existsSync(FREEZE_JS_PATH)) {
    await context.addInitScript({ path: FREEZE_JS_PATH });
  }

  // For micro-tests only: normalize default styles.
  if (applyParityReset && RESET_CSS) {
    await context.addInitScript({
      content: `(() => {
        const css = ${JSON.stringify(RESET_CSS)};
        const style = document.createElement('style');
        style.setAttribute('data-parity-reset', '1');
        style.textContent = css;
        document.documentElement.appendChild(style);
      })();`,
    });
  }

  return context;
}

export function shouldApplyParityResetForHtmlPath(htmlPath) {
  const p = String(htmlPath);
  // Micro-tests are designed to run under the parity reset.
  return p.includes('/websuite/micro/') || p.includes('\\websuite\\micro\\');
}


