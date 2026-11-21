// Gather the built files under their final names, sign what still needs
// signing, verify every signature against the key the app trusts, and write
// latest.json, SHA256SUMS.txt and the release notes.
import { createHash } from 'node:crypto';
import { copyFileSync, createReadStream, existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { PRODUCT_NAME } from './config.mjs';
import { verifyFileSignature } from './minisign.mjs';
import { signFile } from './signing.mjs';
import { assetNames, buildManifest, validateManifest } from './updater-manifest.mjs';

async function sha256(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

export function releaseNotes({ tag, sha }) {
  return [
    `## ${PRODUCT_NAME} ${tag}`,
    '',
    'AI-powered church audio workstation, now as a true native Tauri app.',
    '',
    '### Downloads',
    '- **Windows**: `.exe` installer (or `.msi`)',
    '- **macOS**: Universal `.dmg` (Intel + Apple Silicon)',
    '',
    '### Changes',
    `Built locally from ${sha.slice(0, 7)}. See [commit history](https://github.com/Coolzymccooy/church-studio/commits/${sha}).`,
    '',
  ].join('\n');
}

/** Copy the Mac files into outDir under their release names. */
export function stageMac({ mac, version, outDir }) {
  const names = assetNames(version);
  const updater = path.join(outDir, names.macApp);
  const dmg = path.join(outDir, names.macDmg);
  copyFileSync(mac.updater, updater);
  copyFileSync(mac.updaterSig, `${updater}.sig`);
  copyFileSync(mac.dmg, dmg);
  if (existsSync(mac.verification)) copyFileSync(mac.verification, path.join(outDir, 'macos-verification.json'));
  return Object.freeze({ macApp: updater, macDmg: dmg });
}

/** Rename the copied Windows installers, then sign them on this Mac. */
export async function stageWindows({ windows, version, outDir, signingKey, tauriCwd, logFile }) {
  const names = assetNames(version);
  const staged = {};
  for (const kind of ['windowsNsis', 'windowsMsi']) {
    const target = path.join(outDir, names[kind]);
    renameSync(windows[kind], target);
    await signFile({ filePath: target, signingKey, tauriCwd, logFile });
    staged[kind] = target;
  }
  return Object.freeze(staged);
}

/** Verify each updater artifact against `pubkey`; returns { kind: {name, signature} }. */
export async function verifySigned({ files, pubkey }) {
  const signed = {};
  for (const kind of ['macApp', 'windowsMsi', 'windowsNsis'].filter((k) => files[k])) {
    const signature = readFileSync(`${files[kind]}.sig`, 'utf8').trim();
    await verifyFileSignature({ filePath: files[kind], signatureBase64: signature, pubkeyBase64: pubkey });
    signed[kind] = { name: path.basename(files[kind]), signature };
  }
  return Object.freeze(signed);
}

export async function writeReleaseFiles({ outDir, files, signed, version, tag, sha, repo, requireAll }) {
  const notes = releaseNotes({ tag, sha });
  const notesFile = path.join(outDir, 'release-notes.md');
  writeFileSync(notesFile, notes);
  const manifest = buildManifest({ version, notes, pubDate: new Date().toISOString(), repo, tag, signed });
  const manifestFile = path.join(outDir, 'latest.json');
  writeFileSync(manifestFile, `${JSON.stringify(manifest, null, 2)}\n`);
  const uploads = [
    ...Object.values(files).flatMap((file) => (existsSync(`${file}.sig`) ? [file, `${file}.sig`] : [file])),
    manifestFile,
  ];
  const problems = validateManifest(manifest, { assetNamesOnRelease: uploads.map((f) => path.basename(f)), requireAll });
  const sums = await Promise.all(uploads.map(async (file) => `${await sha256(file)}  ${path.basename(file)}`));
  writeFileSync(path.join(outDir, 'SHA256SUMS.txt'), `${sums.join('\n')}\n`);
  return Object.freeze({ notesFile, manifestFile, uploads: [...uploads, path.join(outDir, 'SHA256SUMS.txt')], problems });
}
