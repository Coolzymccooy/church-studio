import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto';
import { mkdtempSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { parsePublicKey, parseSignature, verifyFileSignature } from './minisign.mjs';

// The key embedded in src-tauri/tauri.conf.json.
const APP_PUBKEY = 'dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE5OTgzQjNBOUJDNTgzRkMKUldUOGc4V2JPanVZcVFXWHYrbmFPYWZ1OXJjN3BUT2VDakNQRmpPaWFJRERLZVpVU1loWDlCSXYK';

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

function tempFile(content) {
  const file = path.join(mkdtempSync(path.join(os.tmpdir(), 'minisign-test-')), 'artifact.bin');
  writeFileSync(file, content);
  return file;
}

test('parses the key the app trusts', () => {
  assert.equal(parsePublicKey(APP_PUBKEY).keyIdHex, 'A9983B3A9BC583FC');
});

test('accepts a valid prehashed signature', async () => {
  const key = makeKey();
  const content = randomBytes(4096);
  const result = await verifyFileSignature({ filePath: tempFile(content), signatureBase64: signBytes(key, content), pubkeyBase64: key.pubkey });
  assert.equal(result.trustedComment, 'timestamp:1\tfile:x');
});

test('rejects a file changed after signing', async () => {
  const key = makeKey();
  const signature = signBytes(key, Buffer.from('original'));
  await assert.rejects(
    verifyFileSignature({ filePath: tempFile(Buffer.from('tampered')), signatureBase64: signature, pubkeyBase64: key.pubkey }),
    /does not match/,
  );
});

test('rejects a signature from a different key', async () => {
  const content = Buffer.from('installer');
  const signature = signBytes(makeKey(), content);
  await assert.rejects(
    verifyFileSignature({ filePath: tempFile(content), signatureBase64: signature, pubkeyBase64: APP_PUBKEY }),
    /trusts key A9983B3A9BC583FC/,
  );
});

test('rejects an edited trusted comment', async () => {
  const key = makeKey();
  const content = Buffer.from('installer');
  const lines = Buffer.from(signBytes(key, content), 'base64').toString().split('\n');
  const edited = b64Text([lines[0], lines[1], 'trusted comment: forged', lines[3], ''].join('\n'));
  await assert.rejects(
    verifyFileSignature({ filePath: tempFile(content), signatureBase64: edited, pubkeyBase64: key.pubkey }),
    /Trusted comment/,
  );
});

test('rejects text that is not a minisign signature', () => {
  assert.throws(() => parseSignature(b64Text('hello\nworld\n')), /not a minisign/);
});
