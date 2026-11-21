import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import {
  NDI_APPLE_TEAM_ID,
  PLATFORMS,
  defaultCacheDir,
  isUniversal,
  pickFile,
  platformFor,
  signedByNdiOnWindows,
  signedByNdiTeam,
} from './ndi-runtime-plan.mjs';

test('bundles on macOS (any arch) and Windows x64 only', () => {
  assert.equal(platformFor('darwin', 'arm64'), 'macos');
  assert.equal(platformFor('darwin', 'x64'), 'macos');
  assert.equal(platformFor('win32', 'x64'), 'windows-x64');
  assert.equal(platformFor('win32', 'arm64'), null);
  assert.equal(platformFor('linux', 'x64'), null);
});

test('downloads come from NDI over https', () => {
  for (const plan of Object.values(PLATFORMS)) {
    assert.match(plan.url, /^https:\/\/downloads\.ndi\.tv\//);
    assert.ok(plan.files.some((f) => f.library), `${plan.dir} ships a library`);
    assert.ok(plan.files.some((f) => /licen[cs]es?\.txt$/i.test(f.out) && !f.optional), `${plan.dir} ships NDI's licence file`);
  }
});

test('output names match what the Rust loader and Tauri config expect', () => {
  assert.deepEqual(PLATFORMS.macos.files.find((f) => f.library).out, 'libndi.dylib');
  assert.deepEqual(PLATFORMS['windows-x64'].files.find((f) => f.library).out, 'Processing.NDI.Lib.x64.dll');
});

test('cache dir: override, then per-OS default', () => {
  assert.equal(defaultCacheDir({ NDI_DOWNLOAD_CACHE: '/c' }, 'darwin', '/h'), '/c');
  assert.equal(defaultCacheDir({}, 'darwin', '/h'), path.join('/h', 'Library', 'Caches', 'tiwaton-ndi'));
  assert.equal(defaultCacheDir({ LOCALAPPDATA: 'L' }, 'win32', 'H'), path.join('L', 'tiwaton-ndi', 'cache'));
  assert.equal(defaultCacheDir({}, 'linux', '/h'), path.join('/h', '.cache', 'tiwaton-ndi'));
});

test('pickFile prefers the platform folder, then the shortest path', () => {
  const spec = PLATFORMS.macos.files[0];
  const files = [
    'NDI SDK for Apple/examples/x/libndi.dylib',
    'NDI SDK for Apple/lib/iOS/libndi.dylib.txt',
    'NDI SDK for Apple/lib/macOS/libndi.dylib',
    'NDI SDK for Apple/bin/libndi.dylib',
  ];
  assert.equal(pickFile(spec, files), 'NDI SDK for Apple/lib/macOS/libndi.dylib');
  assert.equal(pickFile(spec, ['b/libndi.dylib', 'a/libndi.dylib', 'long/er/libndi.dylib']), 'a/libndi.dylib');
  assert.equal(pickFile(spec, ['x/other.dylib']), null);
});

test('pickFile on the Windows SDK layout', () => {
  const dll = PLATFORMS['windows-x64'].files[0];
  const files = ['app/Bin/x86/Processing.NDI.Lib.x86.dll', 'app/Bin/x64/Processing.NDI.Lib.x64.dll'];
  assert.equal(pickFile(dll, files), 'app/Bin/x64/Processing.NDI.Lib.x64.dll');
});

test('universal check needs both Apple Silicon and Intel', () => {
  assert.equal(isUniversal('x86_64 arm64\n'), true);
  assert.equal(isUniversal('arm64'), false);
  assert.equal(isUniversal('x86_64'), false);
});

test('signature checks', () => {
  assert.equal(signedByNdiTeam(`Identifier=libndi\nTeamIdentifier=${NDI_APPLE_TEAM_ID}\n`), true);
  assert.equal(signedByNdiTeam('TeamIdentifier=ABCDE12345'), false);
  assert.equal(signedByNdiTeam(`TeamIdentifier=${NDI_APPLE_TEAM_ID}X`), false);
  assert.equal(signedByNdiOnWindows('CN="Vizrt NDI AB", O="Vizrt NDI AB", C=SE'), true);
  assert.equal(signedByNdiOnWindows('CN=Someone Else'), false);
});
