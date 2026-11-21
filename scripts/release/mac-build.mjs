// Build, test and verify the macOS universal bundle in a dedicated job
// worktree (~/tailnet-jobs/church-studio-release-mac), never in a checkout
// someone may be editing. Mirrors the macos-latest job in tauri-build.yml.
import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { MAC, PRODUCT_NAME } from './config.mjs';
import { run } from './exec.mjs';

export function macBundleDir(jobDir = MAC.jobDir) {
  return path.join(jobDir, 'src-tauri', 'target', MAC.target, 'release', 'bundle');
}

/** Create the job worktree on first use, then move it to `sha`. */
export async function prepareMacWorktree({ repoRoot, sha, jobDir = MAC.jobDir, logFile }) {
  if (!existsSync(jobDir)) {
    await run('git', ['-C', repoRoot, 'worktree', 'add', '--detach', jobDir, sha], { logFile });
  } else if (!existsSync(path.join(jobDir, '.git'))) {
    throw new Error(`${jobDir} exists but is not a git worktree; move it aside and rerun`);
  } else {
    await run('git', ['-C', jobDir, 'checkout', '--force', '--detach', sha], { logFile });
  }
  await run('npm', ['ci', '--no-audit', '--no-fund'], { cwd: jobDir, logFile });
  return jobDir;
}

function findOne(dir, predicate, what) {
  const match = existsSync(dir) ? readdirSync(dir).find(predicate) : undefined;
  if (!match) throw new Error(`No ${what} in ${dir}`);
  return path.join(dir, match);
}

export function locateMacArtifacts({ version, jobDir = MAC.jobDir }) {
  const bundle = macBundleDir(jobDir);
  const app = path.join(bundle, 'macos', `${PRODUCT_NAME}.app`);
  if (!existsSync(app)) throw new Error(`Missing ${app}`);
  return Object.freeze({
    app,
    updater: findOne(path.join(bundle, 'macos'), (f) => f.endsWith('.app.tar.gz'), 'updater .app.tar.gz'),
    updaterSig: findOne(path.join(bundle, 'macos'), (f) => f.endsWith('.app.tar.gz.sig'), 'updater signature'),
    dmg: findOne(path.join(bundle, 'dmg'), (f) => f.endsWith('.dmg') && f.includes(version), `DMG for ${version}`),
    verification: path.join(jobDir, 'macos-verification.json'),
  });
}

/**
 * npm ci → ndi:fetch → tauri build (universal, updater-signed) → frontend + native tests
 * → verify-macos.py (architectures, entitlements, 10 s native launch).
 */
export async function buildMac({ jobDir = MAC.jobDir, signingKey, skipTests, logFile, onStep }) {
  const env = {
    ...process.env,
    ...signingKey.env,
    // Ad-hoc sign unless a real identity is configured, as CI does for test builds.
    APPLE_SIGNING_IDENTITY: process.env.APPLE_SIGNING_IDENTITY || '-',
  };
  // The app bundles the NDI runtime (src-tauri/ndi-runtime); build.rs stops without it.
  onStep('ndi:fetch');
  await run('npm', ['run', 'ndi:fetch'], { cwd: jobDir, logFile });
  onStep('tauri build --target universal-apple-darwin');
  await run('npx', ['tauri', 'build', '--target', MAC.target], { cwd: jobDir, env, logFile });
  if (!skipTests) {
    onStep('npm test');
    await run('npm', ['test'], { cwd: jobDir, logFile });
    onStep('cargo test --locked --all-targets');
    await run('cargo', ['test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--all-targets'], { cwd: jobDir, logFile });
  }
  onStep('verify-macos.py');
  const app = path.join(macBundleDir(jobDir), 'macos', `${PRODUCT_NAME}.app`);
  await run('python3', ['scripts/verify-macos.py', app], { cwd: jobDir, logFile });
}
