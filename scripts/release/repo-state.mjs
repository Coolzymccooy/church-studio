// What is being released: version, commit, and whether the repo is in a
// state that may be published.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { capture, run } from './exec.mjs';

export async function repoRoots(cwd = process.cwd()) {
  const repoRoot = await capture('git', ['-C', cwd, 'rev-parse', '--show-toplevel']);
  const commonDir = await capture('git', ['-C', repoRoot, 'rev-parse', '--path-format=absolute', '--git-common-dir']);
  return Object.freeze({ repoRoot, mainRoot: path.dirname(commonDir) });
}

function cargoVersion(text) {
  const pkg = text.split(/^\[/m).find((section) => section.startsWith('package]'));
  return pkg?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

/** All three version fields must agree, or the updater and installers disagree. */
export function readVersions(repoRoot) {
  const read = (file) => readFileSync(path.join(repoRoot, file), 'utf8');
  const tauriConf = JSON.parse(read('src-tauri/tauri.conf.json'));
  return Object.freeze({
    tauri: tauriConf.version,
    npm: JSON.parse(read('package.json')).version,
    cargo: cargoVersion(read('src-tauri/Cargo.toml')),
    pubkey: tauriConf.plugins?.updater?.pubkey,
    endpoints: tauriConf.plugins?.updater?.endpoints ?? [],
  });
}

export function versionProblems(versions) {
  const { tauri, npm, cargo } = versions;
  return tauri === npm && tauri === cargo
    ? []
    : [`version mismatch: tauri.conf.json ${tauri}, package.json ${npm}, Cargo.toml ${cargo}`];
}

/** Facts about the checkout; `problems` block a publish, a dry run only warns. */
export async function inspectRepo({ repoRoot, tag, repo }) {
  const sha = await capture('git', ['-C', repoRoot, 'rev-parse', 'HEAD']);
  const dirty = await capture('git', ['-C', repoRoot, 'status', '--porcelain', '--untracked-files=no']);
  await run('git', ['-C', repoRoot, 'fetch', '--quiet', 'origin'], { allowFail: true, quiet: true });
  const onOrigin = await capture('git', ['-C', repoRoot, 'branch', '-r', '--contains', sha], { allowFail: true });
  const remoteTag = await capture('git', ['-C', repoRoot, 'ls-remote', '--tags', 'origin', `refs/tags/${tag}`], { allowFail: true });
  const auth = await run('gh', ['auth', 'status', '--hostname', 'github.com'], { capture: true, allowFail: true, quiet: true });
  const release = await run('gh', ['release', 'view', tag, '--repo', repo, '--json', 'tagName'], { capture: true, allowFail: true, quiet: true });
  const problems = [
    auth.code !== 0 && 'gh is not logged in to github.com (run `gh auth login`)',
    dirty && 'tracked files have uncommitted changes (the build uses the committed HEAD only)',
    !onOrigin && `HEAD ${sha.slice(0, 7)} is not on origin; push it first so the tag can point at it`,
    remoteTag && `tag ${tag} already exists on origin`,
    release.code === 0 && `a GitHub release named ${tag} already exists`,
  ].filter(Boolean);
  return Object.freeze({ sha, onMaster: /origin\/master\b/.test(onOrigin), problems });
}
