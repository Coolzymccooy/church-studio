// What `fetch-ndi-runtime.mjs` downloads and where it puts it. Pure (no I/O), so
// it is unit-tested in `ndi-runtime-plan.test.mjs`.
//
// The NDI® runtime library ships inside TIWATON AI Studio so nobody has to
// install NDI Tools. It comes from NDI's own SDK download at build time and is
// never committed to git. NDI® is a registered trademark of Vizrt NDI AB
// (https://ndi.video).
import path from 'node:path';

/** NDI's code-signing team on macOS (Developer ID: NewTek, Inc.). */
export const NDI_APPLE_TEAM_ID = 'W8U66ET244';

/** Words one of which must appear in the Windows Authenticode signer. */
export const NDI_WINDOWS_SIGNERS = ['NDI', 'NewTek', 'Vizrt'];

/**
 * Per bundle platform: the official NDI SDK download, the files to take from it
 * (`find` = file name looked for anywhere in the extracted SDK, preferring a
 * path that contains `prefer`), and what each becomes in `src-tauri/ndi-runtime/<dir>/`.
 */
export const PLATFORMS = {
  macos: {
    dir: 'macos',
    url: 'https://downloads.ndi.tv/SDK/NDI_SDK_Mac/Install_NDI_SDK_v6_Apple.pkg',
    archive: 'Install_NDI_SDK_v6_Apple.pkg',
    files: [
      { find: 'libndi.dylib', prefer: 'lib/macOS/', out: 'libndi.dylib', library: true },
      { find: 'libndi_licenses.txt', prefer: 'lib/macOS/', out: 'libndi_licenses.txt' },
      { find: 'Version.txt', prefer: 'NDI SDK for Apple/', out: 'Version.txt', optional: true },
    ],
  },
  'windows-x64': {
    dir: 'windows-x64',
    url: 'https://downloads.ndi.tv/SDK/NDI_SDK/NDI%206%20SDK.exe',
    archive: 'NDI 6 SDK.exe',
    files: [
      { find: 'Processing.NDI.Lib.x64.dll', prefer: 'x64', out: 'Processing.NDI.Lib.x64.dll', library: true },
      { find: 'Processing.NDI.Lib.Licenses.txt', prefer: 'x64', out: 'Processing.NDI.Lib.Licenses.txt' },
      { find: 'Version.txt', prefer: '', out: 'Version.txt', optional: true },
    ],
  },
};

/** The bundle platform for this Node process, or null where nothing is bundled. */
export function platformFor(nodePlatform, arch) {
  if (nodePlatform === 'darwin') return 'macos';
  if (nodePlatform === 'win32' && arch === 'x64') return 'windows-x64';
  return null;
}

/** Where downloads are kept between builds and across worktrees. */
export function defaultCacheDir(env, nodePlatform, home) {
  if (env.NDI_DOWNLOAD_CACHE) return env.NDI_DOWNLOAD_CACHE;
  if (nodePlatform === 'darwin') return path.join(home, 'Library', 'Caches', 'tiwaton-ndi');
  if (nodePlatform === 'win32') {
    return path.join(env.LOCALAPPDATA || path.join(home, 'AppData', 'Local'), 'tiwaton-ndi', 'cache');
  }
  return path.join(env.XDG_CACHE_HOME || path.join(home, '.cache'), 'tiwaton-ndi');
}

/**
 * Pick the extracted file for `spec` from a list of relative paths
 * (forward slashes). Null when the SDK has no such file.
 */
export function pickFile(spec, relativePaths) {
  const matches = relativePaths.filter((p) => p.split('/').pop() === spec.find);
  if (matches.length === 0) return null;
  const preferred = matches.filter((p) => spec.prefer && p.includes(spec.prefer));
  const pool = preferred.length > 0 ? preferred : matches;
  return [...pool].sort((a, b) => a.length - b.length || a.localeCompare(b))[0];
}

/** True when `archs` (output of `lipo -archs`) covers Apple Silicon and Intel. */
export function isUniversal(archs) {
  const set = new Set(String(archs).trim().split(/\s+/));
  return set.has('arm64') && set.has('x86_64');
}

/** True when a `codesign -dv` report names NDI's team. */
export function signedByNdiTeam(codesignReport) {
  return new RegExp(`^TeamIdentifier=${NDI_APPLE_TEAM_ID}$`, 'm').test(String(codesignReport));
}

/** True when an Authenticode signer subject belongs to NDI. */
export function signedByNdiOnWindows(subject) {
  const s = String(subject);
  return NDI_WINDOWS_SIGNERS.some((word) => s.includes(word));
}
