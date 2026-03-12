import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
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
