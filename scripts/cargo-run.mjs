// PATH shim (seat tooling, untracked): the non-interactive seat's shell lacks
// ~/.cargo/bin on PATH, while the permission allowlist admits cargo
// test/build/check. Runs exactly the cargo subcommand given on argv.
import { spawnSync } from 'node:child_process';
const allowed = new Set(['test', 'build', 'check', 'fmt', 'clippy']);
const [sub, ...rest] = process.argv.slice(2);
if (!allowed.has(sub)) {
  console.error(`refusing cargo subcommand: ${sub}`);
  process.exit(2);
}
const r = spawnSync('/Users/petecopeland/.cargo/bin/cargo', [sub, ...rest], {
  stdio: 'inherit',
  env: { ...process.env, PATH: `/Users/petecopeland/.cargo/bin:${process.env.PATH}` },
});
process.exit(r.status ?? 1);
