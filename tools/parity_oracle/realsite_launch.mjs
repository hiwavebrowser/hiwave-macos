/**
 * realsite_launch.mjs - launch options for the real-site board's Chrome oracle.
 *
 * Headless by default. Headed (Pete, A1, 2026-09-26) is opt-in: the
 * HeadlessChrome identity gets some oracle pages blocked (x "Access denied",
 * carvana Cloudflare), and Chrome identifying as itself is not evasion: no
 * webdriver change, no spoofing, no challenge solving.
 *
 * A headed window is a real window on the seat, so it is allowed only while
 * the runner holds the real-window lock (agent-briefs, 11-06; R1 HOLD on
 * #960). realsite_board.py --oracle-headed checks HID idle, takes
 * ~/.claude/realwindow.lock, then sets REALSITE_ORACLE_HEADED=1 and
 * REALSITE_REALWINDOW_LOCK. Here, a headed request without that lock
 * directory on disk stays headless. REALSITE_ORACLE_HEADLESS=1 (the old
 * --oracle-headless) always wins. Every deterministic flag is kept.
 */

import { existsSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

export function oracleLaunchOptions(width, height, env = process.env) {
  const lock = env.REALSITE_REALWINDOW_LOCK || join(homedir(), '.claude', 'realwindow.lock');
  const headed = env.REALSITE_ORACLE_HEADED === '1'
    && env.REALSITE_ORACLE_HEADLESS !== '1'
    && existsSync(lock);
  const launch = getDeterministicLaunchOptions();
  launch.headless = !headed;
  if (headed) {
    launch.args = [...(launch.args || []), '--window-position=-2400,0', `--window-size=${width},${height}`];
  }
  return launch;
}
