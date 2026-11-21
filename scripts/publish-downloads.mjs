#!/usr/bin/env node
// Put an already staged release on https://downloads.tiwaton.co.uk (Cloudflare R2).
//   node scripts/publish-downloads.mjs <artifacts folder> --dry-run   show what would go up
//   node scripts/publish-downloads.mjs <artifacts folder>             upload, verify, switch over
// The folder is the one release-local.mjs prints ("artifacts: …"). How-to: docs/downloads-host.md
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { DOWNLOADS } from './release/config.mjs';
import { publishDownloads, stageDownloads, verifyManifestSignatures } from './release/downloads-publish.mjs';
import { loadEnvFiles } from './release/env-file.mjs';
import { readVersions, repoRoots } from './release/repo-state.mjs';

const { values: args, positionals } = parseArgs({
  allowPositionals: true,
  options: { 'dry-run': { type: 'boolean', default: false }, partial: { type: 'boolean', default: false } },
});

async function main() {
  const [dir] = positionals;
  if (!dir) throw new Error('usage: node scripts/publish-downloads.mjs <artifacts folder> [--dry-run] [--partial]');
  const outDir = path.resolve(dir);
  const requireAll = !args.partial;
  if (args['dry-run']) {
    const staged = stageDownloads({ outDir, requireAll });
    const { repoRoot } = await repoRoots();
    const signatureProblems = await verifyManifestSignatures({ manifest: staged.manifest, outDir, pubkey: readVersions(repoRoot).pubkey });
    const problems = [...staged.problems, ...signatureProblems];
    console.log(`${staged.version} → ${DOWNLOADS.host} (bucket ${DOWNLOADS.bucket}), DRY RUN`);
    staged.plan.forEach((item) => console.log(`  ${item.key}  [${item.cacheControl}]`));
    problems.forEach((problem) => console.log(`  problem: ${problem}`));
    if (!problems.length) console.log('  signatures verify against tauri.conf.json');
    process.exit(problems.length ? 1 : 0);
  }
  const { repoRoot, mainRoot } = await repoRoots();
  const env = { ...process.env, ...loadEnvFiles([path.join(mainRoot, '.env.local'), path.join(repoRoot, '.env.local')]) };
  const { pubkey } = readVersions(repoRoot);
  const result = await publishDownloads({ outDir, env, pubkey, requireAll, onStep: (m) => console.log(`[downloads] ${m}`) });
  console.log(`\n✓ ${result.version} is live: ${Object.values(result.downloads.files).map((f) => f.url).join('\n  ')}`);
}

main().catch((error) => { console.error(`\n✗ ${error.message}`); process.exit(1); });
