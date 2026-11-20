#!/usr/bin/env node
// Release church-studio from the two laptops, with no GitHub Actions.
//   node scripts/release-local.mjs --dry-run     build, sign, verify; publish nothing
//   node scripts/release-local.mjs               same, then publish v<version>
// How-to: docs/release-local.md
import { existsSync, mkdirSync, statSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import readline from 'node:readline/promises';
import { parseArgs } from 'node:util';
import { GITHUB_REPO, MAC, PRODUCT_NAME } from './release/config.mjs';
import { run } from './release/exec.mjs';
import { buildMac, locateMacArtifacts, prepareMacWorktree } from './release/mac-build.mjs';
import { checkDraft, checkLiveManifest, createDraft, publishDraft } from './release/publish.mjs';
import { inspectRepo, readVersions, repoRoots, versionProblems } from './release/repo-state.mjs';
import { generateTestKey, keyCandidates, resolveSigningKey } from './release/signing.mjs';
import { stageMac, stageWindows, verifySigned, writeReleaseFiles } from './release/stage.mjs';
import { buildWindows, cleanWindowsJob, fetchWindowsArtifacts, preflightWindows } from './release/windows-build.mjs';

const USAGE = `Usage: node scripts/release-local.mjs [options]
  --dry-run         build, sign and verify both platforms; publish nothing
  --yes             publish without the type-the-tag prompt
  --tag <vX.Y.Z>    defaults to v<version from tauri.conf.json>
  --skip-mac        dry run only: skip the macOS build
  --skip-windows    dry run only: skip the Windows build
  --skip-tests      skip npm test and cargo test (not allowed when publishing)
  --clean-windows   remove the ajibowo job folder afterwards
  --help`;

const { values: args } = parseArgs({
  options: {
    'dry-run': { type: 'boolean', default: false },
    yes: { type: 'boolean', default: false },
    tag: { type: 'string' },
    'skip-mac': { type: 'boolean', default: false },
    'skip-windows': { type: 'boolean', default: false },
    'skip-tests': { type: 'boolean', default: false },
    'clean-windows': { type: 'boolean', default: false },
    help: { type: 'boolean', default: false },
  },
});

const started = Date.now();
const stamp = () => new Date().toISOString().slice(11, 19);
const say = (label) => (message) => console.log(`[${label}] ${stamp()} ${message}`);
const fail = (message) => { console.error(`\n✗ ${message}`); process.exit(1); };

function heartbeat(label, logFile) {
  const timer = setInterval(() => {
    const size = existsSync(logFile) ? `${Math.round(statSync(logFile).size / 1024)} KB log` : 'no log yet';
    say(label)(`still running (${Math.round((Date.now() - started) / 60000)} min, ${size})`);
  }, 120000);
  return () => clearInterval(timer);
}

async function withHeartbeat(label, logFile, task) {
  const stop = heartbeat(label, logFile);
  try {
    return await task();
  } finally {
    stop();
  }
}

async function showTailnetLoad() {
  const script = path.join(os.homedir(), '.claude', 'bin', 'tailnet-load.mjs');
  if (existsSync(script)) await run('node', [script], { allowFail: true });
}

/** The Mac worktree also provides the Tauri CLI that signs everything. */
async function prepareMac({ repoRoot, sha, signingKey, logFile }) {
  say('mac')(`job worktree ${MAC.jobDir} @ ${sha.slice(0, 7)}, npm ci`);
  await prepareMacWorktree({ repoRoot, sha, logFile });
  return signingKey ?? generateTestKey({ tauriCwd: MAC.jobDir, logFile });
}

async function macTask({ signingKey, version, logFile }) {
  const step = say('mac');
  await buildMac({ signingKey, skipTests: args['skip-tests'], logFile, onStep: step });
  step('done');
  return locateMacArtifacts({ version });
}

async function windowsTask({ repoRoot, sha, version, workDir, logFile }) {
  const step = say('win');
  const { freeGb } = await preflightWindows();
  step(`ajibowo reachable, ${freeGb} GB free, MSVC Rust toolchain found`);
  await buildWindows({ repoRoot, sha, workDir, logFile, onStep: step });
  step('copying installers back with scp');
  const files = await fetchWindowsArtifacts({ version, destDir: workDir, logFile });
  step('done');
  return files;
}

function reportFailure(label, logFile, error) {
  console.error(`\n✗ ${label} failed: ${error.message}`);
  if (error.tail) console.error(`--- last lines of ${logFile} ---\n${error.tail.split('\n').slice(-40).join('\n')}`);
}

async function confirmPublish(tag) {
  if (args.yes) return true;
  if (!process.stdin.isTTY) fail('Publishing needs --yes when there is no terminal to confirm in');
  const prompt = readline.createInterface({ input: process.stdin, output: process.stdout });
  const answer = await prompt.question(`\nType ${tag} to publish it as the latest release: `);
  prompt.close();
  return answer.trim() === tag;
}

async function publish({ tag, sha, staged, endpoint, version }) {
  const step = say('publish');
  step(`creating draft ${tag} with ${staged.uploads.length} files`);
  await createDraft({ repo: GITHUB_REPO, tag, sha, title: `${PRODUCT_NAME} ${tag}`, notesFile: staged.notesFile, files: staged.uploads });
  const problems = await checkDraft({ repo: GITHUB_REPO, tag, files: staged.uploads });
  if (problems.length) fail(`Draft ${tag} is incomplete; it was left as a draft:\n  ${problems.join('\n  ')}`);
  step('draft verified; publishing and marking latest');
  await publishDraft({ repo: GITHUB_REPO, tag });
  const live = await checkLiveManifest({ endpoint, version });
  step(live ? `updater endpoint now serves ${version}` : `WARNING: ${endpoint} does not show ${version} yet; check it by hand`);
}

async function main() {
  if (args.help) { console.log(USAGE); return; }
  const dryRun = args['dry-run'];
  if (!dryRun && (args['skip-mac'] || args['skip-windows'] || args['skip-tests'])) {
    fail('--skip-mac, --skip-windows and --skip-tests are for dry runs; a release needs both platforms, tested');
  }
  const { repoRoot, mainRoot } = await repoRoots();
  const versions = readVersions(repoRoot);
  const version = versions.tauri;
  const tag = args.tag ?? `v${version}`;
  const problems = [...versionProblems(versions), ...(tag === `v${version}` ? [] : [`tag ${tag} does not match version ${version}`])];
  const state = await inspectRepo({ repoRoot, tag, repo: GITHUB_REPO });
  const allProblems = [...problems, ...state.problems, ...(state.onMaster ? [] : ['HEAD is not on origin/master'])];
  console.log(`${PRODUCT_NAME} ${tag} from ${state.sha.slice(0, 7)} — ${dryRun ? 'DRY RUN (nothing is published)' : 'RELEASE'}`);
  allProblems.forEach((problem) => console.log(`  ${dryRun ? 'warning' : 'blocker'}: ${problem}`));
  if (!dryRun && allProblems.length) fail('Fix the blockers above, or use --dry-run');

  const signingKey = resolveSigningKey({ env: process.env, repoRoot, mainRoot });
  if (!signingKey && !dryRun) {
    fail(`No updater signing key. Put it at one of:\n  ${keyCandidates({ repoRoot, mainRoot }).join('\n  ')}\nor set TAURI_SIGNING_PRIVATE_KEY.`);
  }
  console.log(`  signing key: ${signingKey ? signingKey.source : 'not found — a throwaway test key will sign this dry run'}`);
  await showTailnetLoad();

  const outDir = path.join(MAC.artifactsRoot, `${tag}-${state.sha.slice(0, 7)}`);
  const workDir = path.join(outDir, 'work');
  const logs = { mac: path.join(outDir, 'logs', 'mac.log'), win: path.join(outDir, 'logs', 'windows.log') };
  mkdirSync(path.dirname(logs.mac), { recursive: true });
  mkdirSync(workDir, { recursive: true });
  console.log(`  artifacts: ${outDir}\n  logs: ${path.dirname(logs.mac)}\n`);

  const key = await prepareMac({ repoRoot, sha: state.sha, signingKey, logFile: logs.mac })
    .catch((error) => { reportFailure('macOS setup', logs.mac, error); return fail('Cannot build or sign without the Mac job worktree'); });
  const [macResult, winResult] = await Promise.allSettled([
    args['skip-mac'] ? Promise.resolve(null) : withHeartbeat('mac', logs.mac, () => macTask({ signingKey: key, version, logFile: logs.mac })),
    args['skip-windows']
      ? Promise.resolve(null)
      : withHeartbeat('win', logs.win, () => windowsTask({ repoRoot, sha: state.sha, version, workDir, logFile: logs.win })),
  ]);
  if (macResult.status === 'rejected') reportFailure('macOS', logs.mac, macResult.reason);
  if (winResult.status === 'rejected') reportFailure('Windows', logs.win, winResult.reason);

  const pubkey = key.isTestKey ? key.pubkey : versions.pubkey;
  const macFiles = macResult.value ? stageMac({ mac: macResult.value, version, outDir }) : {};
  const winFiles = winResult.value
    ? await stageWindows({ windows: winResult.value, version, outDir, signingKey: key, tauriCwd: MAC.jobDir, logFile: logs.mac })
    : {};
  const files = { ...macFiles, ...winFiles };
  const signed = await verifySigned({ files, pubkey });
  say('sign')(`verified ${Object.keys(signed).length} updater signatures against ${key.isTestKey ? 'the throwaway test key' : 'the key in tauri.conf.json'}`);
  const staged = await writeReleaseFiles({ outDir, files, signed, version, tag, sha: state.sha, repo: GITHUB_REPO, requireAll: !dryRun });
  staged.problems.forEach((problem) => console.log(`  latest.json: ${problem}`));
  if (args['clean-windows'] && !args['skip-windows']) await cleanWindowsJob({ logFile: logs.win });

  console.log('\nRelease files:');
  staged.uploads.forEach((file) => console.log(`  ${(statSync(file).size / 1048576).toFixed(1).padStart(7)} MB  ${path.basename(file)}`));
  const buildFailed = macResult.status === 'rejected' || winResult.status === 'rejected';
  if (dryRun) {
    console.log(`\nDry run finished in ${Math.round((Date.now() - started) / 60000)} min. Nothing was published.`);
    if (key.isTestKey) console.log('Signed with a throwaway key: install the real key before releasing (docs/release-local.md).');
    process.exit(buildFailed || staged.problems.length ? 1 : 0);
  }
  if (buildFailed || staged.problems.length) fail('Not publishing: a build or the updater manifest failed (see above)');
  if (!(await confirmPublish(tag))) fail('Not confirmed; nothing was published');
  await publish({ tag, sha: state.sha, staged, endpoint: versions.endpoints[0], version });
}

main().catch((error) => fail(error.stack ?? error.message));
