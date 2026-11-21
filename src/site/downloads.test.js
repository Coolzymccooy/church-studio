import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DOWNLOADS_MANIFEST_URL,
  detectPlatform,
  downloadOptions,
  formatSize,
  latestUrl,
  parseDownloads,
} from './downloads.js';

const MAC_UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15';
const WIN_UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36';
const IPHONE_UA = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1';
const ANDROID_UA = 'Mozilla/5.0 (Linux; Android 15; SM-X920) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36';
const LINUX_UA = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0 Safari/537.36';

const HOST = 'https://downloads.tiwaton.co.uk';

test('detects macOS and Windows desktops from the user agent', () => {
  assert.equal(detectPlatform({ userAgent: MAC_UA, maxTouchPoints: 0 }), 'mac');
  assert.equal(detectPlatform({ userAgent: WIN_UA }), 'windows');
});

test('prefers userAgentData.platform when the browser gives it', () => {
  assert.equal(detectPlatform({ userAgent: LINUX_UA, userAgentData: { platform: 'macOS', mobile: false } }), 'mac');
  assert.equal(detectPlatform({ userAgent: MAC_UA, userAgentData: { platform: 'Windows', mobile: false } }), 'windows');
});

test('phones, tablets and Linux get no single file', () => {
  assert.equal(detectPlatform({ userAgent: IPHONE_UA, maxTouchPoints: 5 }), null);
  assert.equal(detectPlatform({ userAgent: ANDROID_UA, maxTouchPoints: 10 }), null);
  assert.equal(detectPlatform({ userAgent: LINUX_UA }), null);
  assert.equal(detectPlatform({ userAgent: WIN_UA, userAgentData: { platform: 'Windows', mobile: true } }), null);
});

test('iPadOS Safari says Macintosh but has touch: no Mac file', () => {
  assert.equal(detectPlatform({ userAgent: MAC_UA, maxTouchPoints: 5 }), null);
});

test('missing or odd navigator values do not throw', () => {
  assert.equal(detectPlatform(), null);
  assert.equal(detectPlatform({}), null);
  assert.equal(detectPlatform({ userAgent: 42, userAgentData: 'x' }), null);
});

test('stable latest URLs live on the downloads host, never github.com', () => {
  assert.equal(latestUrl('mac'), `${HOST}/studio/latest/mac.dmg`);
  assert.equal(latestUrl('windows'), `${HOST}/studio/latest/windows.exe`);
  assert.equal(DOWNLOADS_MANIFEST_URL, `${HOST}/studio/downloads.json`);
  assert.throws(() => latestUrl('linux'), /unknown platform/);
});

test('formats sizes in MB with one decimal', () => {
  assert.equal(formatSize(15_380_367), '14.7 MB');
  assert.equal(formatSize(4_741_770), '4.5 MB');
  assert.equal(formatSize(0), null);
  assert.equal(formatSize(undefined), null);
  assert.equal(formatSize(-5), null);
});

const MANIFEST = {
  version: '1.5.1',
  pub_date: '2026-11-21T15:28:17.000Z',
  files: {
    mac: { url: `${HOST}/studio/v1.5.1/TIWATON.AI.Studio_1.5.1_universal.dmg`, size: 15_380_367 },
    windows: { url: `${HOST}/studio/v1.5.1/TIWATON.AI.Studio_1.5.1_x64-setup.exe`, size: 4_741_770 },
  },
};

test('parses a good downloads manifest', () => {
  const parsed = parseDownloads(MANIFEST);
  assert.equal(parsed.version, '1.5.1');
  assert.equal(parsed.files.mac.size, 15_380_367);
  assert.ok(Object.isFrozen(parsed));
});

test('rejects manifests that point off the downloads host or are malformed', () => {
  assert.equal(parseDownloads(null), null);
  assert.equal(parseDownloads({ ...MANIFEST, version: 'latest' }), null);
  const offHost = { ...MANIFEST, files: { ...MANIFEST.files, mac: { url: 'https://github.com/x/y.dmg', size: 1 } } };
  assert.equal(parseDownloads(offHost).files.mac, undefined);
  const lookalike = { ...MANIFEST, files: { windows: { url: `${HOST}.evil.example/a.exe`, size: 1 } } };
  assert.equal(parseDownloads(lookalike).files.windows, undefined);
  const insecure = { ...MANIFEST, files: { windows: { url: 'http://downloads.tiwaton.co.uk/studio/a.exe', size: 1 } } };
  assert.equal(parseDownloads(insecure).files.windows, undefined);
  const badSize = { ...MANIFEST, files: { mac: { url: MANIFEST.files.mac.url, size: 'big' } } };
  assert.equal(parseDownloads(badSize).files.mac.size, null);
});

test('download options use the manifest when it loaded', () => {
  const options = downloadOptions(parseDownloads(MANIFEST));
  assert.deepEqual(options.map((o) => o.platform), ['mac', 'windows']);
  assert.equal(options[0].href, MANIFEST.files.mac.url);
  assert.equal(options[0].label, 'macOS');
  assert.equal(options[0].size, '14.7 MB');
  assert.equal(options[0].version, '1.5.1');
});

test('download options fall back to the stable URLs without a manifest', () => {
  const options = downloadOptions(null);
  assert.equal(options[0].href, `${HOST}/studio/latest/mac.dmg`);
  assert.equal(options[1].href, `${HOST}/studio/latest/windows.exe`);
  assert.equal(options[0].size, null);
  assert.equal(options[0].version, null);
  options.forEach((o) => assert.ok(!o.href.includes('github.com')));
});

test('a platform missing from the manifest keeps the stable URL and shows no version', () => {
  const macOnly = parseDownloads({ ...MANIFEST, files: { mac: MANIFEST.files.mac } });
  const windows = downloadOptions(macOnly).find((o) => o.platform === 'windows');
  assert.equal(windows.href, `${HOST}/studio/latest/windows.exe`);
  assert.equal(windows.version, null);
});
