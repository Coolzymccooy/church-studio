// Verify Tauri updater signatures (minisign format) with Node's own crypto,
// so a release is checked the same way the installed app will check it.
//
// Public key (tauri.conf.json, base64 of a text file):
//   untrusted comment: …
//   base64( "Ed" | keyId[8] | ed25519PublicKey[32] )
// Signature (.sig file, base64 of a text file):
//   untrusted comment: …
//   base64( "ED" | keyId[8] | ed25519Signature[64] )   "ED" = BLAKE2b-512 prehashed
//   trusted comment: …
//   base64( ed25519Signature over signature[64] + trustedComment )
import { createHash, createPublicKey, verify } from 'node:crypto';
import { createReadStream } from 'node:fs';

const PUBKEY_ALG = 'Ed';
const PREHASHED = 'ED';
const LEGACY = 'Ed';

function decodeBase64Text(value, what) {
  const text = Buffer.from(String(value).trim(), 'base64').toString('utf8');
  const lines = text.split(/\r?\n/).filter((line) => line.length > 0);
  if (lines.length < 2 || !lines[0].startsWith('untrusted comment:')) {
    throw new Error(`${what} is not a minisign document`);
  }
  return lines;
}

function keyIdHex(keyId) {
  return Buffer.from(keyId).reverse().toString('hex').toUpperCase();
}

export function parsePublicKey(pubkeyBase64) {
  const [, keyLine] = decodeBase64Text(pubkeyBase64, 'Public key');
  const raw = Buffer.from(keyLine, 'base64');
  if (raw.length !== 42 || raw.subarray(0, 2).toString('latin1') !== PUBKEY_ALG) {
    throw new Error('Public key has an unexpected length or algorithm');
  }
  return Object.freeze({ keyId: raw.subarray(2, 10), keyIdHex: keyIdHex(raw.subarray(2, 10)), key: raw.subarray(10) });
}

export function parseSignature(signatureBase64) {
  const lines = decodeBase64Text(signatureBase64, 'Signature');
  if (lines.length < 4 || !lines[2].startsWith('trusted comment: ')) {
    throw new Error('Signature is missing its trusted comment');
  }
  const raw = Buffer.from(lines[1], 'base64');
  const algorithm = raw.subarray(0, 2).toString('latin1');
  if (raw.length !== 74 || (algorithm !== PREHASHED && algorithm !== LEGACY)) {
    throw new Error('Signature has an unexpected length or algorithm');
  }
  return Object.freeze({
    algorithm,
    keyId: raw.subarray(2, 10),
    keyIdHex: keyIdHex(raw.subarray(2, 10)),
    signature: raw.subarray(10),
    trustedComment: lines[2].slice('trusted comment: '.length),
    globalSignature: Buffer.from(lines[3], 'base64'),
  });
}

function ed25519Key(rawPublicKey) {
  return createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: Buffer.from(rawPublicKey).toString('base64url') }, format: 'jwk' });
}

async function blake2b512(filePath) {
  const hash = createHash('blake2b512');
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest();
}

async function readAll(filePath) {
  const chunks = [];
  for await (const chunk of createReadStream(filePath)) chunks.push(chunk);
  return Buffer.concat(chunks);
}

/** Throws unless `signatureBase64` is a valid signature of the file by `pubkeyBase64`. */
export async function verifyFileSignature({ filePath, signatureBase64, pubkeyBase64 }) {
  const publicKey = parsePublicKey(pubkeyBase64);
  const sig = parseSignature(signatureBase64);
  if (!publicKey.keyId.equals(sig.keyId)) {
    throw new Error(`Signed by key ${sig.keyIdHex}, but the app trusts key ${publicKey.keyIdHex}`);
  }
  const key = ed25519Key(publicKey.key);
  const message = sig.algorithm === PREHASHED ? await blake2b512(filePath) : await readAll(filePath);
  if (!verify(null, message, key, sig.signature)) {
    throw new Error(`Signature does not match the contents of ${filePath}`);
  }
  const globalMessage = Buffer.concat([sig.signature, Buffer.from(sig.trustedComment, 'utf8')]);
  if (!verify(null, globalMessage, key, sig.globalSignature)) {
    throw new Error('Trusted comment signature is invalid');
  }
  return Object.freeze({ keyIdHex: sig.keyIdHex, trustedComment: sig.trustedComment });
}
