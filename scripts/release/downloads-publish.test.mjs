import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { mkdtempSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { stageDownloads, verifyManifestSignatures } from './downloads-publish.mjs';
import { assetNames } from './updater-manifest.mjs';

const b64Text = (text) => Buffer.from(text).toString('base64');

function makeKey() {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const raw = Buffer.from(publicKey.export({ format: 'jwk' }).x, 'base64url');
  const keyId = randomBytes(8);
  const pubkey = b64Text(`untrusted comment: test key\n${Buffer.concat([Buffer.from('Ed'), keyId, raw]).toString('base64')}\n`);
  return { privateKey, keyId, pubkey };
}

function signBytes({ privateKey, keyId }, content, comment = 'timestamp:1\tfile:x') {
  const digest = createHash('blake2b512').update(content).digest();
  const signature = sign(null, digest, privateKey);
  const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
  return b64Text([
    'untrusted comment: signature from tauri secret key',
    Buffer.concat([Buffer.from('ED'), keyId, signature]).toString('base64'),
    `trusted comment: ${comment}`,
    global.toString('base64'),
    '',
  ].join('\n'));
}

/** A staged mac-only release folder signed with `key`. */
function stagedRelease(key) {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'downloads-publish-'));
  const names = assetNames('1.5.1');
  const app = Buffer.from('app bundle bytes');
  writeFileSync(path.join(dir, names.macApp), app);
  writeFileSync(path.join(dir, `${names.macApp}.sig`), signBytes(key, app));
  writeFileSync(path.join(dir, names.macDmg), 'dmg bytes');
  const url = `https://github.com/o/r/releases/download/v1.5.1/${names.macApp}`;
  writeFileSync(path.join(dir, 'latest.json'), JSON.stringify({
    version: '1.5.1', notes: '', pub_date: '2026-11-21T00:00:00Z',
    platforms: { 'darwin-aarch64': { signature: signBytes(key, app), url } },
  }));
  return dir;
}

test('signatures made with the trusted key pass', async () => {
  const key = makeKey();
  const dir = stagedRelease(key);
  const { manifest } = stageDownloads({ outDir: dir, requireAll: false });
  assert.deepEqual(await verifyManifestSignatures({ manifest, outDir: dir, pubkey: key.pubkey }), []);
});

test('a dry run signed with a throwaway key is refused', async () => {
  const dir = stagedRelease(makeKey());
  const { manifest } = stageDownloads({ outDir: dir, requireAll: false });
  const problems = await verifyManifestSignatures({ manifest, outDir: dir, pubkey: makeKey().pubkey });
  assert.equal(problems.length, 1);
  assert.match(problems[0], /darwin-aarch64/);
});

test('a missing public key is refused', async () => {
  const dir = stagedRelease(makeKey());
  const { manifest } = stageDownloads({ outDir: dir, requireAll: false });
  const problems = await verifyManifestSignatures({ manifest, outDir: dir, pubkey: '' });
  assert.match(problems.join(), /public key/);
});
