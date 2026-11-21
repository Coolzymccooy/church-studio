// Upload to the Cloudflare R2 bucket with the aws CLI (S3 API), and prove each file is public.
// Keys come from .env.local: R2_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY.
import { createHash } from 'node:crypto';
import { createReadStream, statSync } from 'node:fs';
import { redact } from './env-file.mjs';
import { run } from './exec.mjs';

const ACCOUNT_ID = /^[0-9a-f]{32}$/;
const REQUIRED = Object.freeze(['R2_ACCOUNT_ID', 'R2_ACCESS_KEY_ID', 'R2_SECRET_ACCESS_KEY']);

/** Returns { problems, endpoint, childEnv, secrets }. childEnv is a fresh env for the aws CLI. */
export function r2Settings(env) {
  const missing = REQUIRED.filter((name) => !env[name]);
  const problems = missing.map((name) => `${name} is not set (add it to .env.local)`);
  if (env.R2_ACCOUNT_ID && !ACCOUNT_ID.test(env.R2_ACCOUNT_ID)) problems.push('R2_ACCOUNT_ID is not a 32-character account id');
  const childEnv = Object.freeze({
    PATH: env.PATH ?? process.env.PATH,
    HOME: env.HOME ?? process.env.HOME,
    AWS_ACCESS_KEY_ID: env.R2_ACCESS_KEY_ID ?? '',
    AWS_SECRET_ACCESS_KEY: env.R2_SECRET_ACCESS_KEY ?? '',
    AWS_REGION: 'auto',
    // Keep ~/.aws profiles out of it, and skip the CRC checksums R2 rejects (aws CLI 2.23+).
    AWS_CONFIG_FILE: '/dev/null',
    AWS_SHARED_CREDENTIALS_FILE: '/dev/null',
    AWS_REQUEST_CHECKSUM_CALCULATION: 'when_required',
    AWS_RESPONSE_CHECKSUM_VALIDATION: 'when_required',
  });
  return Object.freeze({
    problems,
    endpoint: `https://${env.R2_ACCOUNT_ID}.r2.cloudflarestorage.com`,
    childEnv,
    secrets: [env.R2_ACCESS_KEY_ID, env.R2_SECRET_ACCESS_KEY],
  });
}

/** One PUT per file; R2 accepts single uploads up to 5 GB and ours are ~15 MB. */
export async function putObject({ item, bucket, settings, logFile }) {
  const args = [
    's3api', 'put-object', '--endpoint-url', settings.endpoint, '--bucket', bucket,
    '--key', item.key, '--body', item.source,
    '--content-type', item.contentType, '--cache-control', item.cacheControl,
    ...(item.contentDisposition ? ['--content-disposition', item.contentDisposition] : []),
  ];
  try {
    await run('aws', args, { env: settings.childEnv, logFile, quiet: !logFile });
  } catch (error) {
    throw new Error(redact(`upload of ${item.key} failed: ${error.message}\n${error.tail ?? ''}`, settings.secrets));
  }
}

export async function sha256File(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

/**
 * Download `url` past the CDN cache and compare it with the local file.
 * Returns a list of problems; empty means the public copy is byte-identical.
 */
export async function verifyPublic({ url, file, bust }) {
  const expected = { size: statSync(file).size, sha256: await sha256File(file) };
  const response = await fetch(`${url}?v=${encodeURIComponent(bust)}`, { cache: 'no-store', signal: AbortSignal.timeout(120000) });
  if (!response.ok || !response.body) return [`${url} answered HTTP ${response.status}`];
  const hash = createHash('sha256');
  let size = 0;
  for await (const chunk of response.body) {
    hash.update(chunk);
    size += chunk.length;
  }
  const problems = [];
  if (size !== expected.size) problems.push(`${url}: ${size} bytes, expected ${expected.size}`);
  if (hash.digest('hex') !== expected.sha256) problems.push(`${url}: sha256 does not match the local file`);
  return problems;
}
