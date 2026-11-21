// Put a staged release (the folder release-local.mjs writes) on Tiwaton's download host.
// Installers go up first and are downloaded back and compared byte for byte; only then do
// latest.json (installed apps) and downloads.json (the website) switch to the new version.
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { DOWNLOADS } from './config.mjs';
import { buildDownloadsJson, hostedUrl, kindOfAsset, rehostManifest, uploadPlan } from './downloads-host.mjs';
import { putObject, r2Settings, verifyPublic } from './r2.mjs';
import { assetNames, validateManifest } from './updater-manifest.mjs';

const MANIFEST_KEYS = new Set([`${DOWNLOADS.prefix}/latest.json`, `${DOWNLOADS.prefix}/downloads.json`]);

function releaseFiles(outDir, version) {
  const candidates = Object.values(assetNames(version)).flatMap((name) => [name, `${name}.sig`]);
  return [...candidates, 'SHA256SUMS.txt']
    .filter((name) => existsSync(path.join(outDir, name)))
    .map((name) => Object.freeze({ path: path.join(outDir, name), name, size: statSync(path.join(outDir, name)).size }));
}

/** Writes hosted/latest.json and hosted/downloads.json in outDir; returns the upload plan. */
export function stageDownloads({ outDir, requireAll = true }) {
  const manifest = JSON.parse(readFileSync(path.join(outDir, 'latest.json'), 'utf8'));
  const { version } = manifest;
  const files = releaseFiles(outDir, version);
  const hosted = rehostManifest(manifest);
  const problems = validateManifest(hosted, { assetNamesOnRelease: files.map((f) => f.name), requireAll });
  const byKind = Object.fromEntries(files.map((file) => [kindOfAsset(version, file.name), file]).filter(([kind]) => kind));
  if (!byKind.macDmg && requireAll) problems.push('no .dmg to offer on the website');
  if (!byKind.windowsNsis && requireAll) problems.push('no setup .exe to offer on the website');
  const downloads = buildDownloadsJson({ version, pubDate: manifest.pub_date, files: byKind });

  const hostedDir = path.join(outDir, 'hosted');
  mkdirSync(hostedDir, { recursive: true });
  const hostedManifestPath = path.join(hostedDir, 'latest.json');
  const downloadsJsonPath = path.join(hostedDir, 'downloads.json');
  writeFileSync(hostedManifestPath, `${JSON.stringify(hosted, null, 2)}\n`);
  writeFileSync(downloadsJsonPath, `${JSON.stringify(downloads, null, 2)}\n`);
  const plan = uploadPlan({ version, files, hostedManifestPath, downloadsJsonPath });
  return Object.freeze({ version, plan, problems, downloads });
}

async function uploadAndVerify({ items, settings, logFile, onStep, bust }) {
  for (const item of items) {
    onStep(`upload ${item.key}`);
    await putObject({ item, bucket: DOWNLOADS.bucket, settings, logFile });
  }
  const problems = [];
  for (const item of items.filter((i) => !i.key.endsWith('.sig') && !i.key.endsWith('.txt'))) {
    problems.push(...await verifyPublic({ url: hostedUrl(item.key), file: item.source, bust }));
  }
  return problems;
}

/** Upload, verify and switch over. Throws (before the manifests move) if any public copy is wrong. */
export async function publishDownloads({ outDir, env, logFile, onStep = () => {}, requireAll = true }) {
  const staged = stageDownloads({ outDir, requireAll });
  if (staged.problems.length) throw new Error(`download host: ${staged.problems.join('; ')}`);
  const settings = r2Settings(env);
  if (settings.problems.length) throw new Error(`download host: ${settings.problems.join('; ')}`);
  const bust = `${staged.version}-${Date.now()}`;
  const files = staged.plan.filter((item) => !MANIFEST_KEYS.has(item.key));
  const manifests = staged.plan.filter((item) => MANIFEST_KEYS.has(item.key));

  const fileProblems = await uploadAndVerify({ items: files, settings, logFile, onStep, bust });
  if (fileProblems.length) throw new Error(`public copies are wrong; manifests NOT switched:\n  ${fileProblems.join('\n  ')}`);
  onStep(`${files.length} files live and byte-identical; switching latest.json and downloads.json`);
  const manifestProblems = await uploadAndVerify({ items: manifests, settings, logFile, onStep, bust });
  if (manifestProblems.length) throw new Error(`manifests did not read back:\n  ${manifestProblems.join('\n  ')}`);
  return Object.freeze({ version: staged.version, downloads: staged.downloads, uploaded: staged.plan.length });
}
