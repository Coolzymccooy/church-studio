import assert from 'node:assert/strict';
import test from 'node:test';
import {
  buildDownloadsJson,
  contentTypeFor,
  hostedUrl,
  kindOfAsset,
  rehostManifest,
  uploadPlan,
  versionKey,
} from './downloads-host.mjs';
import { assetNames, validateManifest } from './updater-manifest.mjs';

const HOST = 'https://downloads.tiwaton.co.uk';
const VERSION = '1.5.1';
const names = assetNames(VERSION);

const githubManifest = Object.freeze({
  version: VERSION,
  notes: 'notes',
  pub_date: '2026-11-21T15:28:17.000Z',
  platforms: {
    'darwin-aarch64': { signature: 'mac-sig', url: `https://github.com/o/r/releases/download/v1.5.1/${names.macApp}` },
    'windows-x86_64-nsis': { signature: 'nsis-sig', url: `https://github.com/o/r/releases/download/v1.5.1/${names.windowsNsis}` },
  },
});

test('versioned keys sit under the product prefix', () => {
  assert.equal(versionKey(VERSION, names.macDmg), 'studio/v1.5.1/TIWATON.AI.Studio_1.5.1_universal.dmg');
  assert.equal(hostedUrl('studio/latest.json'), `${HOST}/studio/latest.json`);
  assert.equal(hostedUrl('studio/v1.5.1/a b.dmg'), `${HOST}/studio/v1.5.1/a%20b.dmg`);
});

test('rehosting keeps version, notes and signatures and swaps every URL to our host', () => {
  const hosted = rehostManifest(githubManifest);
  assert.equal(hosted.version, VERSION);
  assert.equal(hosted.platforms['darwin-aarch64'].signature, 'mac-sig');
  assert.equal(hosted.platforms['darwin-aarch64'].url, `${HOST}/studio/v1.5.1/${names.macApp}`);
  assert.equal(hosted.platforms['windows-x86_64-nsis'].url, `${HOST}/studio/v1.5.1/${names.windowsNsis}`);
  assert.ok(Object.values(hosted.platforms).every((p) => !p.url.includes('github.com')));
  assert.equal(githubManifest.platforms['darwin-aarch64'].url.includes('github.com'), true, 'input is not mutated');
  const problems = validateManifest(hosted, { assetNamesOnRelease: [names.macApp, names.windowsNsis], requireAll: false });
  assert.deepEqual(problems, []);
});

test('recognises each release asset by name', () => {
  assert.equal(kindOfAsset(VERSION, names.macDmg), 'macDmg');
  assert.equal(kindOfAsset(VERSION, `${names.windowsNsis}.sig`), null);
  assert.equal(kindOfAsset(VERSION, 'TIWATON.AI.Studio_1.5.0_universal.dmg'), null);
});

test('downloads.json lists the dmg and the setup exe with sizes on our host', () => {
  const json = buildDownloadsJson({
    version: VERSION,
    pubDate: githubManifest.pub_date,
    files: { macDmg: { name: names.macDmg, size: 15380367 }, windowsNsis: { name: names.windowsNsis, size: 4741770 } },
  });
  assert.deepEqual(json.files.mac, { name: names.macDmg, size: 15380367, url: `${HOST}/studio/v1.5.1/${names.macDmg}` });
  assert.equal(json.files.windows.url, `${HOST}/studio/v1.5.1/${names.windowsNsis}`);
  assert.equal(json.version, VERSION);
});

test('downloads.json leaves out a platform that was not built', () => {
  const json = buildDownloadsJson({ version: VERSION, pubDate: githubManifest.pub_date, files: { macDmg: { name: names.macDmg, size: 1 } } });
  assert.deepEqual(Object.keys(json.files), ['mac']);
});

test('content types', () => {
  assert.equal(contentTypeFor('a.dmg'), 'application/x-apple-diskimage');
  assert.equal(contentTypeFor('a-setup.exe'), 'application/vnd.microsoft.portable-executable');
  assert.equal(contentTypeFor('a.msi'), 'application/x-msi');
  assert.equal(contentTypeFor('a.app.tar.gz'), 'application/gzip');
  assert.equal(contentTypeFor('a.sig'), 'text/plain; charset=utf-8');
  assert.equal(contentTypeFor('latest.json'), 'application/json');
  assert.equal(contentTypeFor('x.bin'), 'application/octet-stream');
});

test('upload order: versioned files, then latest aliases, then the manifests last', () => {
  const files = [names.macDmg, names.macApp, `${names.macApp}.sig`, names.windowsNsis, `${names.windowsNsis}.sig`]
    .map((name) => ({ path: `/out/${name}`, name }));
  const plan = uploadPlan({ version: VERSION, files, hostedManifestPath: '/out/hosted/latest.json', downloadsJsonPath: '/out/hosted/downloads.json' });
  const keys = plan.map((item) => item.key);
  assert.deepEqual(keys.slice(-2), ['studio/latest.json', 'studio/downloads.json']);
  const firstAlias = keys.indexOf('studio/latest/mac.dmg');
  assert.ok(firstAlias > keys.indexOf(`studio/v1.5.1/${names.windowsNsis}.sig`), 'aliases come after every versioned file');
  assert.ok(keys.includes('studio/latest/windows.exe'));

  const versioned = plan.find((item) => item.key === `studio/v1.5.1/${names.macDmg}`);
  assert.match(versioned.cacheControl, /immutable/);
  const alias = plan.find((item) => item.key === 'studio/latest/mac.dmg');
  assert.equal(alias.source, `/out/${names.macDmg}`);
  assert.equal(alias.contentDisposition, `attachment; filename="${names.macDmg}"`);
  assert.match(alias.cacheControl, /max-age=300/);
  const manifest = plan.find((item) => item.key === 'studio/latest.json');
  assert.match(manifest.cacheControl, /max-age=0/);
  assert.equal(manifest.contentType, 'application/json');
});

test('upload plan refuses a file name with a path or the wrong version', () => {
  const bad = [{ path: '/out/x', name: '../x.dmg' }];
  assert.throws(() => uploadPlan({ version: VERSION, files: bad, hostedManifestPath: 'a', downloadsJsonPath: 'b' }), /unexpected/);
  const other = [{ path: '/out/x', name: 'TIWATON.AI.Studio_1.4.0_universal.dmg' }];
  assert.throws(() => uploadPlan({ version: VERSION, files: other, hostedManifestPath: 'a', downloadsJsonPath: 'b' }), /unexpected/);
});
