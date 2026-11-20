// Find the updater signing key on this Mac, sign files with the Tauri CLI,
// and fall back to a throwaway key for dry runs when the real key is absent.
// The key never leaves this machine: Windows builds unsigned, the Mac signs.
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { KEY_FILE_NAME } from './config.mjs';
import { run } from './exec.mjs';

/**
 * Where the real key may be, in order. A worktree has no git-ignored files,
 * so the main checkout is searched too.
 */
export function keyCandidates({ repoRoot, mainRoot, home = os.homedir() }) {
  const unique = new Set([
    path.join(repoRoot, 'src-tauri', 'keys', KEY_FILE_NAME),
    path.join(mainRoot, 'src-tauri', 'keys', KEY_FILE_NAME),
    path.join(home, '.tauri', KEY_FILE_NAME),
  ]);
  return [...unique];
}

/** Returns { source, env } where env holds what the Tauri CLI needs, or null. */
export function resolveSigningKey({ env, repoRoot, mainRoot }) {
  const password = env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? '';
  const fromEnv = env.TAURI_SIGNING_PRIVATE_KEY;
  if (fromEnv) {
    const isPath = existsSync(fromEnv);
    return Object.freeze({
      source: isPath ? fromEnv : 'TAURI_SIGNING_PRIVATE_KEY (inline)',
      env: { TAURI_SIGNING_PRIVATE_KEY: isPath ? readFileSync(fromEnv, 'utf8').trim() : fromEnv, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password },
      isTestKey: false,
    });
  }
  const found = [env.TAURI_SIGNING_PRIVATE_KEY_PATH, ...keyCandidates({ repoRoot, mainRoot })]
    .filter(Boolean)
    .find((candidate) => existsSync(candidate));
  if (!found) return null;
  return Object.freeze({
    source: found,
    env: { TAURI_SIGNING_PRIVATE_KEY: readFileSync(found, 'utf8').trim(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: password },
    isTestKey: false,
  });
}

/** Dry runs only: a fresh key pair in a temp folder. Its signatures are never published. */
export async function generateTestKey({ tauriCwd, logFile }) {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'church-studio-test-key-'));
  const keyPath = path.join(dir, 'test-updater.key');
  await run('npx', ['tauri', 'signer', 'generate', '--ci', '-p', '', '-w', keyPath], { cwd: tauriCwd, logFile });
  return Object.freeze({
    source: `${keyPath} (throwaway test key)`,
    env: { TAURI_SIGNING_PRIVATE_KEY: readFileSync(keyPath, 'utf8').trim(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' },
    pubkey: readFileSync(`${keyPath}.pub`, 'utf8').trim(),
    isTestKey: true,
  });
}

/** Writes `<file>.sig` next to the file and returns its contents. */
export async function signFile({ filePath, signingKey, tauriCwd, logFile }) {
  const env = { ...process.env, ...signingKey.env };
  delete env.TAURI_SIGNING_PRIVATE_KEY_PATH;
  await run('npx', ['tauri', 'signer', 'sign', filePath], { cwd: tauriCwd, env, logFile });
  return readFileSync(`${filePath}.sig`, 'utf8').trim();
}
