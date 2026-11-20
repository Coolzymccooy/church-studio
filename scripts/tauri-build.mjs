import { existsSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';

const repoRoot = process.cwd();
const keyPath = path.join(repoRoot, 'src-tauri', 'keys', 'tiwaton-updater.key');
const env = { ...process.env };

if (!env.TAURI_SIGNING_PRIVATE_KEY && existsSync(keyPath)) {
  env.TAURI_SIGNING_PRIVATE_KEY = keyPath;
}

if (!env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
  env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '';
}

// Release builds ship the latest NDI runtime (the NDI SDK licence asks for the
// current SDK at distribution time): pick up a new NDI release if there is one.
const fetched = spawnSync(process.execPath, [path.join(repoRoot, 'scripts', 'fetch-ndi-runtime.mjs'), '--refresh'], {
  cwd: repoRoot,
  env,
  stdio: 'inherit',
});
if (fetched.status !== 0) {
  process.exit(fetched.status ?? 1);
}

const extraArgs = process.argv.slice(2);
const child = spawn('npx', ['tauri', 'build', ...extraArgs], {
  cwd: repoRoot,
  env,
  shell: true,
  stdio: 'inherit',
});

child.on('exit', (code) => {
  process.exit(code ?? 1);
});
