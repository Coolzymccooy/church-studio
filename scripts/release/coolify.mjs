// Minimal Coolify API client. The token is only ever sent as a header; every
// message that leaves this module goes through redact().
import { redact } from './env-file.mjs';

const TERMINAL = new Set(['finished', 'failed', 'cancelled-by-user', 'cancelled']);

export function coolifyClient({ baseUrl, token }) {
  if (!token) throw new Error('COOLIFY_API_TOKEN is not set (expected in .env.local)');
  const root = `${baseUrl.replace(/\/$/, '')}/api/v1`;
  const request = async (pathAndQuery, { method = 'GET' } = {}) => {
    const response = await fetch(`${root}${pathAndQuery}`, {
      method,
      headers: { Authorization: `Bearer ${token}`, Accept: 'application/json' },
      signal: AbortSignal.timeout(30000),
    });
    const text = await response.text();
    if (!response.ok) {
      throw new Error(redact(`Coolify ${method} ${pathAndQuery} → HTTP ${response.status}: ${text.slice(0, 300)}`, [token]));
    }
    return text ? JSON.parse(text) : {};
  };
  return Object.freeze({
    application: (uuid) => request(`/applications/${uuid}`),
    deploy: (uuid, force) => request(`/deploy?uuid=${encodeURIComponent(uuid)}&force=${force ? 'true' : 'false'}`),
    deployment: (deploymentUuid) => request(`/deployments/${deploymentUuid}`),
    lastDeployment: async (uuid) => (await request(`/deployments/applications/${uuid}?skip=0&take=1`)).deployments?.[0] ?? null,
  });
}

/** Poll until the deployment finishes, fails or times out. */
export async function waitForDeployment({ client, deploymentUuid, timeoutMs = 20 * 60000, intervalMs = 10000, onStatus }) {
  const deadline = Date.now() + timeoutMs;
  let last = '';
  while (Date.now() < deadline) {
    const deployment = await client.deployment(deploymentUuid);
    if (deployment.status !== last) onStatus?.(deployment.status, deployment);
    last = deployment.status;
    if (TERMINAL.has(deployment.status)) return deployment;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`Deployment ${deploymentUuid} did not finish within ${Math.round(timeoutMs / 60000)} min (last status: ${last})`);
}

/** HTTPS GET; TLS errors and non-2xx both count as failures. */
export async function checkHttps(url, { expectText } = {}) {
  const response = await fetch(url, { redirect: 'follow', signal: AbortSignal.timeout(20000) });
  const body = await response.text();
  const problems = [
    !response.ok && `HTTP ${response.status}`,
    !String(response.headers.get('content-type')).includes('text/html') && `content-type ${response.headers.get('content-type')}`,
    expectText && !body.includes(expectText) && `page does not contain "${expectText}"`,
  ].filter(Boolean);
  return Object.freeze({ url, status: response.status, problems });
}
