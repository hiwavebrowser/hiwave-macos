/**
 * realsite_launch.mjs - launch options for the real-site board's Chrome oracle.
 *
 * Headed (Pete, A1, 2026-09-26): the HeadlessChrome identity gets some oracle
 * pages blocked (x "Access denied", carvana Cloudflare). This is Chrome
 * identifying as itself, not evasion: no webdriver change, no spoofing, no
 * challenge solving. The window sits off-screen so it never takes the desktop.
 * REALSITE_ORACLE_HEADLESS=1 (realsite_board.py --oracle-headless) restores
 * headless for before/after readings. Every deterministic flag is kept.
 */

import { getDeterministicLaunchOptions } from './deterministic.mjs';

export function oracleLaunchOptions(width, height, env = process.env) {
  const headless = env.REALSITE_ORACLE_HEADLESS === '1';
  const launch = getDeterministicLaunchOptions();
  launch.headless = headless;
  if (!headless) {
    launch.args = [...(launch.args || []), '--window-position=-2400,0', `--window-size=${width},${height}`];
  }
  return launch;
}
