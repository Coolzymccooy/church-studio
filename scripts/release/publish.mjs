// Publish through the GitHub Releases API with `gh`. This uses no Actions
// minutes. Same order as CI: draft with every asset, check the draft, then
// publish and mark it latest (the "Publish verified release" job).
import { statSync } from 'node:fs';
import path from 'node:path';
import { capture, run } from './exec.mjs';

export async function createDraft({ repo, tag, sha, title, notesFile, files }) {
  await run('gh', ['release', 'create', tag, '--repo', repo, '--draft', '--target', sha,
    '--title', title, '--notes-file', notesFile, ...files]);
}

/** Every local file must be on the draft with the same byte size. */
export async function checkDraft({ repo, tag, files }) {
  const json = await capture('gh', ['release', 'view', tag, '--repo', repo, '--json', 'assets,isDraft']);
  const { assets, isDraft } = JSON.parse(json);
  const remote = new Map(assets.map((asset) => [asset.name, asset.size]));
  const problems = files
    .map((file) => [path.basename(file), statSync(file).size])
    .filter(([name, size]) => remote.get(name) !== size)
    .map(([name, size]) => `${name}: expected ${size} bytes, release has ${remote.get(name) ?? 'nothing'}`);
  if (!isDraft) problems.push(`${tag} is no longer a draft`);
  return problems;
}

export async function publishDraft({ repo, tag }) {
  await run('gh', ['release', 'edit', tag, '--repo', repo, '--draft=false', '--latest']);
}

/** The endpoint installed apps poll must now serve the new version. */
export async function checkLiveManifest({ endpoint, version, attempts = 6 }) {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      const response = await fetch(endpoint, { redirect: 'follow', signal: AbortSignal.timeout(20000) });
      if (response.ok && (await response.json()).version === version) return true;
    } catch {
      // GitHub's redirect can lag a few seconds behind the publish; retry.
    }
    await new Promise((resolve) => setTimeout(resolve, 5000 * attempt));
  }
  return false;
}
