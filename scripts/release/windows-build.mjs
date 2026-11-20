// Build the Windows installers on ajibowo over SSH, in a dedicated job folder
// (C:\Users\segun\tailnet-jobs\church-studio-release), never the main checkout.
// The commit travels as a git bundle over scp, so GitHub is not needed.
// Windows builds WITHOUT updater signing; the Mac signs the installers after
// they are copied back, so the private key never leaves the Mac.
import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { PRODUCT_NAME, SSH_OPTIONS, WINDOWS } from './config.mjs';
import { capture, run } from './exec.mjs';

export function remotePaths({ jobsDir = WINDOWS.jobsDir, jobName = WINDOWS.jobName } = {}) {
  const base = `${jobsDir}\\${jobName}`;
  return Object.freeze({
    jobDir: base,
    bundle: `${base}.bundle`,
    script: `${base}.cmd`,
    config: `${base}.windows.json`,
    bundleDir: `${base}\\src-tauri\\target\\release\\bundle`,
  });
}

/** scp wants forward slashes for a Windows target path. */
export function scpPath(windowsPath) {
  return `${WINDOWS.host}:${windowsPath.replaceAll('\\', '/')}`;
}

// The rustup proxies in %USERPROFILE%\.cargo\bin on ajibowo point at a
// rustup.exe that is not there, so they cannot run. The real toolchain under
// .rustup\toolchains works; put its bin first on PATH and skip the proxies.
export function windowsBuildScript({ sha, paths = remotePaths() }) {
  if (!/^[0-9a-f]{40}$/.test(sha)) throw new Error(`Not a full commit sha: ${sha}`);
  return [
    '@echo off',
    'setlocal EnableExtensions',
    'set "RUSTBIN="',
    'for /d %%T in ("%USERPROFILE%\\.rustup\\toolchains\\stable-*-pc-windows-msvc") do set "RUSTBIN=%%T\\bin"',
    'if not defined RUSTBIN (echo RELEASE-LOCAL: no stable MSVC Rust toolchain in %USERPROFILE%\\.rustup\\toolchains & exit /b 20)',
    'set "PATH=%RUSTBIN%;%PATH%"',
    'set "RUSTUP_TOOLCHAIN="',
    '"%RUSTBIN%\\cargo.exe" --version || exit /b 21',
    `if not exist "${paths.jobDir}\\.git" (git init -q "${paths.jobDir}" || exit /b 22)`,
    `git -C "${paths.jobDir}" fetch -q --force "${paths.bundle}" HEAD || exit /b 23`,
    `git -C "${paths.jobDir}" checkout -q --force --detach ${sha} || exit /b 24`,
    `cd /d "${paths.jobDir}" || exit /b 25`,
    'echo RELEASE-LOCAL: npm ci',
    'call npm ci --no-audit --no-fund || exit /b 26',
    'echo RELEASE-LOCAL: tauri build',
    `call npx tauri build --config "${paths.config}" || exit /b 27`,
    'echo RELEASE-LOCAL-DONE',
    '',
  ].join('\r\n');
}

// Updater artifacts need the private key, which stays on the Mac.
export const WINDOWS_BUILD_CONFIG = Object.freeze({ bundle: { createUpdaterArtifacts: false } });

const ssh = (command, options) => run('ssh', [...SSH_OPTIONS, WINDOWS.host, command], options);
const sshCapture = (command, options) => capture('ssh', [...SSH_OPTIONS, WINDOWS.host, command], options);

/** Reachability, free disk and a working Rust toolchain. Throws with what is missing. */
export async function preflightWindows() {
  const freeGb = Number(await sshCapture('powershell -NoProfile -Command "[math]::Floor((Get-PSDrive C).Free/1GB)"'));
  if (!Number.isFinite(freeGb)) throw new Error('Could not read free disk on ajibowo');
  if (freeGb < WINDOWS.minFreeGb) {
    throw new Error(`ajibowo has ${freeGb} GB free; a release build needs at least ${WINDOWS.minFreeGb} GB`);
  }
  const toolchains = await sshCapture('dir /b "%USERPROFILE%\\.rustup\\toolchains"', { allowFail: true });
  if (!/stable-.*-pc-windows-msvc/.test(toolchains)) {
    throw new Error('ajibowo has no stable MSVC Rust toolchain under %USERPROFILE%\\.rustup\\toolchains');
  }
  return Object.freeze({ freeGb });
}

export async function buildWindows({ repoRoot, sha, workDir, logFile, onStep }) {
  const paths = remotePaths();
  const bundleFile = path.join(workDir, 'church-studio.bundle');
  const scriptFile = path.join(workDir, 'windows-build.cmd');
  const configFile = path.join(workDir, 'windows-build.json');
  onStep('git bundle → ajibowo');
  await run('git', ['-C', repoRoot, 'bundle', 'create', bundleFile, 'HEAD'], { logFile });
  writeFileSync(scriptFile, windowsBuildScript({ sha, paths }));
  writeFileSync(configFile, `${JSON.stringify(WINDOWS_BUILD_CONFIG)}\n`);
  await run('scp', ['-q', ...SSH_OPTIONS, bundleFile, scpPath(paths.bundle)], { logFile });
  await run('scp', ['-q', ...SSH_OPTIONS, scriptFile, scpPath(paths.script)], { logFile });
  await run('scp', ['-q', ...SSH_OPTIONS, configFile, scpPath(paths.config)], { logFile });
  onStep('npm ci + tauri build on ajibowo');
  await ssh(`cmd /c "${paths.script}"`, { logFile });
}

/** Copy the NSIS and MSI installers back into `destDir`; returns local paths. */
export async function fetchWindowsArtifacts({ version, destDir, logFile }) {
  const { bundleDir } = remotePaths();
  const files = {
    windowsNsis: `${bundleDir}\\nsis\\${PRODUCT_NAME}_${version}_x64-setup.exe`,
    windowsMsi: `${bundleDir}\\msi\\${PRODUCT_NAME}_${version}_x64_en-US.msi`,
  };
  const copied = {};
  for (const [kind, remote] of Object.entries(files)) {
    const local = path.join(destDir, path.win32.basename(remote));
    await run('scp', ['-q', ...SSH_OPTIONS, scpPath(remote), local], { logFile });
    copied[kind] = local;
  }
  return Object.freeze(copied);
}

/** Opt-in: remove the job folder and its bundle/script (keeps nothing else). */
export async function cleanWindowsJob({ logFile }) {
  const paths = remotePaths();
  await ssh(`cmd /c rmdir /s /q "${paths.jobDir}" & del /q "${paths.bundle}" "${paths.script}" "${paths.config}"`, { logFile, allowFail: true });
}
