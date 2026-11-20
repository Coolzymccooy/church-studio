// Build and check latest.json, matching the layout tauri-action published for
// v1.5.0 so installed apps behave exactly as before.
//
// The updater looks up `{os}-{arch}-{installer}` first, then `{os}-{arch}`
// (tauri-plugin-updater 2.10, updater.rs get_urls).
import { ASSET_PREFIX } from './config.mjs';

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/** Which updater platform keys each signed asset serves. */
export const PLATFORM_KEYS = Object.freeze({
  macApp: ['darwin-aarch64', 'darwin-x86_64', 'darwin-aarch64-app', 'darwin-x86_64-app'],
  windowsMsi: ['windows-x86_64', 'windows-x86_64-msi'],
  windowsNsis: ['windows-x86_64-nsis'],
});

export const REQUIRED_PLATFORMS = Object.freeze(Object.values(PLATFORM_KEYS).flat());

/** Final, dotted asset names, so the download URL is exactly what GitHub serves. */
export function assetNames(version) {
  return Object.freeze({
    macApp: `${ASSET_PREFIX}_${version}_universal.app.tar.gz`,
    macDmg: `${ASSET_PREFIX}_${version}_universal.dmg`,
    windowsMsi: `${ASSET_PREFIX}_${version}_x64_en-US.msi`,
    windowsNsis: `${ASSET_PREFIX}_${version}_x64-setup.exe`,
  });
}

export function releaseDownloadUrl(repo, tag, assetName) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}`;
}

/**
 * signed: { macApp?: {name, signature}, windowsMsi?: …, windowsNsis?: … }
 * Only the kinds present get platform entries (partial dry runs).
 */
export function buildManifest({ version, notes, pubDate, repo, tag, signed }) {
  const platforms = Object.entries(PLATFORM_KEYS)
    .filter(([kind]) => signed[kind])
    .flatMap(([kind, keys]) => keys.map((key) => [key, {
      signature: signed[kind].signature.trim(),
      url: releaseDownloadUrl(repo, tag, signed[kind].name),
    }]));
  return Object.freeze({ version, notes, pub_date: pubDate, platforms: Object.fromEntries(platforms) });
}

/** Returns a list of problems; empty means the manifest is publishable. */
export function validateManifest(manifest, { assetNamesOnRelease, requireAll = true }) {
  const problems = [];
  if (!SEMVER.test(String(manifest.version))) problems.push(`version "${manifest.version}" is not semver`);
  if (Number.isNaN(Date.parse(manifest.pub_date))) problems.push('pub_date is not a date');
  const entries = Object.entries(manifest.platforms ?? {});
  if (entries.length === 0) problems.push('no platforms');
  if (requireAll) {
    REQUIRED_PLATFORMS.filter((key) => !manifest.platforms?.[key])
      .forEach((key) => problems.push(`missing platform ${key}`));
  }
  entries.forEach(([key, entry]) => {
    if (!entry.signature) problems.push(`${key}: empty signature`);
    if (!String(entry.url).startsWith('https://')) problems.push(`${key}: url is not https`);
    const name = decodeURIComponent(String(entry.url).split('/').pop());
    if (!assetNamesOnRelease.includes(name)) problems.push(`${key}: ${name} is not among the release assets`);
  });
  return problems;
}
