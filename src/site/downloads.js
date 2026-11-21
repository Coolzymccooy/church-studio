// Where the desktop installers live and which one this visitor needs. Pure, so it is unit-tested.
// Files are served from Tiwaton's own host (Cloudflare R2); the site never links to github.com.

export const DOWNLOADS_HOST = 'https://downloads.tiwaton.co.uk';
const PRODUCT_PATH = 'studio';
const BASE = `${DOWNLOADS_HOST}/${PRODUCT_PATH}`;

/** Written by scripts/publish-downloads.mjs on every release. */
export const DOWNLOADS_MANIFEST_URL = `${BASE}/downloads.json`;

const PLATFORMS = Object.freeze({
  mac: Object.freeze({ label: 'macOS', file: 'mac.dmg', kind: '.dmg' }),
  windows: Object.freeze({ label: 'Windows', file: 'windows.exe', kind: '.exe' }),
});

const PLATFORM_ORDER = Object.freeze(['mac', 'windows']);
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const MOBILE_UA = /iPhone|iPad|iPod|Android|Mobile/i;

const text = (value) => (typeof value === 'string' ? value : '');

/**
 * 'mac' | 'windows' | null for a navigator-like object. Phones, tablets and Linux get null:
 * there is no single installer to hand them. iPadOS Safari claims to be a Mac, so a Mac user agent
 * with a touch screen counts as a tablet (no Mac has a touch screen).
 */
export function detectPlatform(nav = {}) {
  const ua = text(nav.userAgent);
  const data = nav.userAgentData && typeof nav.userAgentData === 'object' ? nav.userAgentData : null;
  if (data?.mobile === true || MOBILE_UA.test(ua)) return null;
  const hint = text(data?.platform) || ua;
  if (/Windows|Win64|Win32/i.test(hint)) return 'windows';
  if (/macOS|Macintosh|Mac OS X/i.test(hint)) return Number(nav.maxTouchPoints) > 1 ? null : 'mac';
  return null;
}

/** The stable URL that always serves the newest installer for `platform`. */
export function latestUrl(platform) {
  const entry = PLATFORMS[platform];
  if (!entry) throw new Error(`unknown platform: ${platform}`);
  return `${BASE}/latest/${entry.file}`;
}

export function formatSize(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return null;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

function onHost(url) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'https:' && parsed.origin === DOWNLOADS_HOST;
  } catch {
    return false;
  }
}

function parseFile(entry) {
  if (!entry || typeof entry !== 'object' || !onHost(entry.url)) return null;
  const size = Number.isFinite(entry.size) && entry.size > 0 ? entry.size : null;
  return Object.freeze({ url: entry.url, size });
}

/** A checked copy of downloads.json, or null. Files that point anywhere but our host are dropped. */
export function parseDownloads(json) {
  if (!json || typeof json !== 'object' || !SEMVER.test(text(json.version))) return null;
  const files = PLATFORM_ORDER
    .map((platform) => [platform, parseFile(json.files?.[platform])])
    .filter(([, file]) => file);
  return Object.freeze({ version: json.version, files: Object.freeze(Object.fromEntries(files)) });
}

/** One entry per platform; the manifest's versioned URL when it loaded, else the stable one. */
export function downloadOptions(manifest) {
  return PLATFORM_ORDER.map((platform) => {
    const file = manifest?.files?.[platform];
    return Object.freeze({
      platform,
      label: PLATFORMS[platform].label,
      kind: PLATFORMS[platform].kind,
      href: file?.url ?? latestUrl(platform),
      size: formatSize(file?.size),
      version: file ? manifest.version : null,
    });
  });
}
