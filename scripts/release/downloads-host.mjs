// The layout of Tiwaton's download host, kept pure so it is unit-tested.
//   studio/v<version>/<asset>      every release file, never overwritten (cached for a year)
//   studio/latest/mac.dmg          stable links the website and docs use (cached 5 minutes)
//   studio/latest/windows.exe
//   studio/latest.json             the updater manifest, URLs pointing at studio/v<version>/
//   studio/downloads.json          version and sizes for the website's Download key
import path from 'node:path';
import { DOWNLOADS } from './config.mjs';
import { assetNames } from './updater-manifest.mjs';

const CACHE = Object.freeze({
  versioned: 'public, max-age=31536000, immutable',
  alias: 'public, max-age=300',
  manifest: 'public, max-age=0, must-revalidate',
});

/** Which asset kinds get a stable latest/ alias, and its file name. */
export const LATEST_ALIASES = Object.freeze({ macDmg: 'mac.dmg', windowsNsis: 'windows.exe' });
const SITE_PLATFORM = Object.freeze({ macDmg: 'mac', windowsNsis: 'windows' });

const TYPES = Object.freeze([
  ['.app.tar.gz', 'application/gzip'],
  ['.dmg', 'application/x-apple-diskimage'],
  ['.exe', 'application/vnd.microsoft.portable-executable'],
  ['.msi', 'application/x-msi'],
  ['.sig', 'text/plain; charset=utf-8'],
  ['.json', 'application/json'],
  ['.txt', 'text/plain; charset=utf-8'],
]);

export const versionKey = (version, name) => `${DOWNLOADS.prefix}/v${version}/${name}`;
const productKey = (name) => `${DOWNLOADS.prefix}/${name}`;

export function hostedUrl(key) {
  return `${DOWNLOADS.host}/${key.split('/').map(encodeURIComponent).join('/')}`;
}

export function contentTypeFor(name) {
  return TYPES.find(([suffix]) => name.endsWith(suffix))?.[1] ?? 'application/octet-stream';
}

/** 'macDmg' | 'macApp' | 'windowsNsis' | 'windowsMsi' for a release asset name, else null. */
export function kindOfAsset(version, name) {
  return Object.entries(assetNames(version)).find(([, assetName]) => assetName === name)?.[0] ?? null;
}

/** The GitHub-style latest.json with every URL moved to our host. Returns a new object. */
export function rehostManifest(manifest) {
  const platforms = Object.entries(manifest.platforms ?? {}).map(([key, entry]) => {
    const name = decodeURIComponent(String(entry.url).split('/').pop());
    return [key, { ...entry, url: hostedUrl(versionKey(manifest.version, name)) }];
  });
  return Object.freeze({ ...manifest, platforms: Object.fromEntries(platforms) });
}

/** files: { macDmg?: {name, size}, windowsNsis?: {name, size} } */
export function buildDownloadsJson({ version, pubDate, files }) {
  const entries = Object.entries(SITE_PLATFORM)
    .filter(([kind]) => files[kind])
    .map(([kind, platform]) => [platform, {
      name: files[kind].name,
      size: files[kind].size,
      url: hostedUrl(versionKey(version, files[kind].name)),
    }]);
  return Object.freeze({ version, pub_date: pubDate, files: Object.fromEntries(entries) });
}

function isReleaseFile(version, name) {
  if (path.basename(name) !== name) return false;
  const base = name.endsWith('.sig') ? name.slice(0, -4) : name;
  return kindOfAsset(version, base) !== null || name === 'SHA256SUMS.txt';
}

/**
 * Ordered uploads: every versioned file first, then the latest/ aliases, then latest.json and
 * downloads.json last, so nothing ever points at a file that is not there yet.
 * files: [{ path, name }]
 */
export function uploadPlan({ version, files, hostedManifestPath, downloadsJsonPath }) {
  const unexpected = files.filter((file) => !isReleaseFile(version, file.name));
  if (unexpected.length) throw new Error(`unexpected files for ${version}: ${unexpected.map((f) => f.name).join(', ')}`);
  const versioned = files.map((file) => ({
    source: file.path,
    key: versionKey(version, file.name),
    contentType: contentTypeFor(file.name),
    cacheControl: CACHE.versioned,
  }));
  const aliases = files
    .filter((file) => LATEST_ALIASES[kindOfAsset(version, file.name)])
    .map((file) => ({
      source: file.path,
      key: productKey(`latest/${LATEST_ALIASES[kindOfAsset(version, file.name)]}`),
      contentType: contentTypeFor(file.name),
      cacheControl: CACHE.alias,
      contentDisposition: `attachment; filename="${file.name}"`,
    }));
  const manifests = [
    { source: hostedManifestPath, key: productKey('latest.json'), contentType: 'application/json', cacheControl: CACHE.manifest },
    { source: downloadsJsonPath, key: productKey('downloads.json'), contentType: 'application/json', cacheControl: CACHE.manifest },
  ];
  return Object.freeze([...versioned, ...aliases, ...manifests].map((item) => Object.freeze(item)));
}
