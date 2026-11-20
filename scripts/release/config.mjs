// Fixed facts about where church-studio is built, signed, published and hosted.
// Everything machine-specific lives here so the scripts stay readable.
import os from 'node:os';
import path from 'node:path';

export const GITHUB_REPO = 'Coolzymccooy/church-studio';
export const PRODUCT_NAME = 'TIWATON AI Studio';
// GitHub turns spaces in asset names into dots; we rename before upload so the
// URLs in latest.json are exactly what GitHub serves.
export const ASSET_PREFIX = PRODUCT_NAME.replaceAll(' ', '.');
export const KEY_FILE_NAME = 'tiwaton-updater.key';

export const MAC = Object.freeze({
  jobDir: path.join(os.homedir(), 'tailnet-jobs', 'church-studio-release-mac'),
  artifactsRoot: path.join(os.homedir(), 'tailnet-jobs', 'church-studio-artifacts'),
  target: 'universal-apple-darwin',
});

// ajibowo (Windows). Reached with `ssh ajibowo`, which opens cmd.exe.
export const WINDOWS = Object.freeze({
  host: 'ajibowo',
  jobsDir: 'C:\\Users\\segun\\tailnet-jobs',
  jobName: 'church-studio-release',
  minFreeGb: 6,
});

export const COOLIFY = Object.freeze({
  defaultBaseUrl: 'https://coolify.tiwaton.co.uk',
  appUuid: 'wqomypvsbykpaenipi9ytb1k',
  appName: 'abike-church-studio-web',
  publicUrl: 'https://abike.tiwaton.co.uk',
  branch: 'master',
});

export const SSH_OPTIONS = Object.freeze([
  '-o', 'BatchMode=yes',
  '-o', 'ConnectTimeout=15',
  '-o', 'ServerAliveInterval=30',
  '-o', 'ServerAliveCountMax=10',
]);
