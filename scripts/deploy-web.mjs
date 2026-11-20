#!/usr/bin/env node
// Deploy the web app (abike.tiwaton.co.uk) through the Coolify API, without
// GitHub Actions, then check it over HTTPS.
//   node scripts/deploy-web.mjs --dry-run   check the app, token and site only
//   node scripts/deploy-web.mjs             deploy origin/master and wait for it
//   node scripts/deploy-web.mjs --force     rebuild without Coolify's cache
// Needs COOLIFY_API_TOKEN (deploy permission) in the git-ignored .env.local.
import path from 'node:path';
import process from 'node:process';
import { parseArgs } from 'node:util';
import { COOLIFY } from './release/config.mjs';
import { checkHttps, coolifyClient, waitForDeployment } from './release/coolify.mjs';
import { loadEnvFiles, redact } from './release/env-file.mjs';
import { capture } from './release/exec.mjs';
import { repoRoots } from './release/repo-state.mjs';

const { values: args } = parseArgs({
  options: {
    'dry-run': { type: 'boolean', default: false },
    force: { type: 'boolean', default: false },
    help: { type: 'boolean', default: false },
  },
});

const say = (message) => console.log(`${new Date().toISOString().slice(11, 19)} ${message}`);

async function loadCoolifyEnv() {
  const { repoRoot, mainRoot } = await repoRoots();
  // A worktree has no git-ignored files, so fall back to the main checkout's copy.
  const env = { ...loadEnvFiles([path.join(mainRoot, '.env.local'), path.join(repoRoot, '.env.local')]), ...pick(process.env) };
  return { repoRoot, token: env.COOLIFY_API_TOKEN, baseUrl: env.COOLIFY_BASE_URL || COOLIFY.defaultBaseUrl };
}

function pick(env) {
  return Object.fromEntries(['COOLIFY_API_TOKEN', 'COOLIFY_BASE_URL'].filter((k) => env[k]).map((k) => [k, env[k]]));
}

/** Refuse to deploy if the uuid no longer points at the app we think it does. */
function appProblems(app) {
  return [
    app.name !== COOLIFY.appName && `name is "${app.name}", expected "${COOLIFY.appName}"`,
    !String(app.fqdn).split(',').includes(COOLIFY.publicUrl) && `domain is "${app.fqdn}", expected ${COOLIFY.publicUrl}`,
    app.git_branch !== COOLIFY.branch && `branch is "${app.git_branch}", expected ${COOLIFY.branch}`,
  ].filter(Boolean);
}

async function checkSite() {
  const results = await Promise.all([
    checkHttps(`${COOLIFY.publicUrl}/`, { expectText: 'TIWATON AI Studio' }),
    checkHttps(`${COOLIFY.publicUrl}/?app`),
  ]);
  results.forEach((r) => say(`${r.url} → ${r.status}${r.problems.length ? ` (${r.problems.join(', ')})` : ' ok'}`));
  return results.every((r) => r.problems.length === 0);
}

async function main() {
  if (args.help) { console.log('Usage: node scripts/deploy-web.mjs [--dry-run] [--force]'); return; }
  const { repoRoot, token, baseUrl } = await loadCoolifyEnv();
  const client = coolifyClient({ baseUrl, token });
  const app = await client.application(COOLIFY.appUuid);
  const problems = appProblems(app);
  if (problems.length) throw new Error(`Coolify app ${COOLIFY.appUuid} is not the expected app: ${problems.join('; ')}`);
  say(`${app.name}: ${app.status}, builds ${app.git_repository}@${app.git_branch}`);

  const remoteHead = (await capture('git', ['-C', repoRoot, 'ls-remote', 'origin', `refs/heads/${COOLIFY.branch}`])).split(/\s/)[0];
  say(`origin/${COOLIFY.branch} is ${remoteHead.slice(0, 7)}; Coolify builds that commit from GitHub, not local files`);

  if (args['dry-run']) {
    const last = await client.lastDeployment(COOLIFY.appUuid);
    if (last) say(`last deployment ${last.status}: ${String(last.commit).slice(0, 7)} at ${last.updated_at}${last.commit === remoteHead ? ' (up to date)' : ''}`);
    const ok = await checkSite();
    say(`dry run: no deploy triggered${ok ? '' : '; the live site has problems (above)'}`);
    process.exit(ok ? 0 : 1);
  }

  const queued = await client.deploy(COOLIFY.appUuid, args.force);
  const deploymentUuid = queued.deployments?.[0]?.deployment_uuid;
  if (!deploymentUuid) throw new Error(`Coolify did not queue a deployment: ${JSON.stringify(queued).slice(0, 200)}`);
  say(`queued deployment ${deploymentUuid}`);
  const done = await waitForDeployment({ client, deploymentUuid, onStatus: (status) => say(`deployment ${status}`) });
  if (done.status !== 'finished') throw new Error(`Deployment ended as ${done.status}; see Coolify logs for ${deploymentUuid}`);
  if (done.commit && done.commit !== remoteHead) say(`warning: deployed ${String(done.commit).slice(0, 7)}, origin/${COOLIFY.branch} is ${remoteHead.slice(0, 7)}`);
  if (!(await checkSite())) throw new Error('Deployed, but the HTTPS check failed');
  say(`deployed ${String(done.commit ?? remoteHead).slice(0, 7)} to ${COOLIFY.publicUrl}`);
}

main().catch(async (error) => {
  const { token } = await loadCoolifyEnv().catch(() => ({}));
  console.error(`✗ ${redact(error.message, [token])}`);
  process.exit(1);
});
