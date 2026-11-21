import assert from 'node:assert/strict';
import test from 'node:test';
import { parseEnvText, redact } from './env-file.mjs';
import { versionProblems } from './repo-state.mjs';
import { assetNames, buildManifest, REQUIRED_PLATFORMS, validateManifest } from './updater-manifest.mjs';
import { remotePaths, scpPath, windowsBuildScript } from './windows-build.mjs';

const SHA = '843628998ce638cf68e782f119155d81d672467b';

test('env files keep a pipe inside the value and ignore comments', () => {
  const env = parseEnvText('# note\nCOOLIFY_API_TOKEN=12|abcdefghijkl\nexport A="quoted value"\nnot a line\n');
  assert.equal(env.COOLIFY_API_TOKEN, '12|abcdefghijkl');
  assert.equal(env.A, 'quoted value');
  assert.equal(Object.keys(env).length, 2);
});

test('redact hides a token and its secret half', () => {
  const out = redact('Bearer 12|abcdefghijkl and abcdefghijkl', ['12|abcdefghijkl']);
  assert.equal(out.includes('abcdefghijkl'), false);
});

test('version fields must agree', () => {
  assert.deepEqual(versionProblems({ tauri: '1.5.1', npm: '1.5.1', cargo: '1.5.1' }), []);
  assert.equal(versionProblems({ tauri: '1.5.1', npm: '1.5.0', cargo: '1.5.1' }).length, 1);
});

function fullManifest() {
  const names = assetNames('1.5.1');
  const signed = {
    macApp: { name: names.macApp, signature: 'c2ln' },
    windowsMsi: { name: names.windowsMsi, signature: 'c2ln' },
    windowsNsis: { name: names.windowsNsis, signature: 'c2ln' },
  };
  const manifest = buildManifest({ version: '1.5.1', notes: 'n', pubDate: '2026-11-20T00:00:00Z', repo: 'o/r', tag: 'v1.5.1', signed });
  return { manifest, names: Object.values(names) };
}

test('manifest has every platform key v1.5.0 published, with dotted asset URLs', () => {
  const { manifest, names } = fullManifest();
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [...REQUIRED_PLATFORMS].sort());
  assert.equal(manifest.platforms['windows-x86_64'].url,
    'https://github.com/o/r/releases/download/v1.5.1/TIWATON.AI.Studio_1.5.1_x64_en-US.msi');
  assert.equal(manifest.platforms['windows-x86_64-nsis'].url.endsWith('TIWATON.AI.Studio_1.5.1_x64-setup.exe'), true);
  assert.deepEqual(validateManifest(manifest, { assetNamesOnRelease: names }), []);
});

test('manifest validation catches a missing asset and a missing platform', () => {
  const { manifest } = fullManifest();
  const problems = validateManifest(manifest, { assetNamesOnRelease: [] });
  assert.equal(problems.some((p) => p.includes('not among the release assets')), true);
  const partial = { ...manifest, platforms: { 'darwin-aarch64': manifest.platforms['darwin-aarch64'] } };
  assert.equal(validateManifest(partial, { assetNamesOnRelease: [assetNames('1.5.1').macApp] })
    .some((p) => p.startsWith('missing platform windows-x86_64')), true);
});

test('windows build script uses the real toolchain and the exact commit', () => {
  const script = windowsBuildScript({ sha: SHA });
  assert.match(script, /\.rustup\\toolchains\\stable-\*-pc-windows-msvc/);
  assert.match(script, new RegExp(`checkout -q --force --detach ${SHA}`));
  assert.match(script, /tailnet-jobs\\church-studio-release/);
  assert.equal(script.includes('\r\n'), true);
  assert.throws(() => windowsBuildScript({ sha: 'master' }), /full commit sha/);
});

test('scp paths use forward slashes', () => {
  assert.equal(scpPath(remotePaths().bundle), 'ajibowo:C:/Users/segun/tailnet-jobs/church-studio-release.bundle');
});
