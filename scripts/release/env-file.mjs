// Read KEY=value files (.env.local) without shell semantics.
// Never `source` these: Coolify tokens look like `12|abc…`, and a shell treats
// the `|` as a pipe and runs the secret half as a command.
import { existsSync, readFileSync } from 'node:fs';

const LINE = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/;

function unquote(value) {
  const quoted = value.match(/^(['"])(.*)\1$/);
  return quoted ? quoted[2] : value;
}

export function parseEnvText(text) {
  const entries = text
    .split(/\r?\n/)
    .filter((line) => line.trim() && !line.trim().startsWith('#'))
    .map((line) => line.match(LINE))
    .filter(Boolean)
    .map(([, key, value]) => [key, unquote(value)]);
  return Object.freeze(Object.fromEntries(entries));
}

/** Merge env files in order; later files win. Missing files are skipped. */
export function loadEnvFiles(paths) {
  return paths
    .filter((file) => existsSync(file))
    .reduce((merged, file) => ({ ...merged, ...parseEnvText(readFileSync(file, 'utf8')) }), {});
}

/** Replace any occurrence of a secret (and its `|`-separated halves) in text. */
export function redact(text, secrets) {
  return secrets
    .filter((secret) => typeof secret === 'string' && secret.length >= 8)
    .flatMap((secret) => [secret, ...secret.split('|').filter((part) => part.length >= 8)])
    .reduce((out, secret) => out.split(secret).join('[redacted]'), String(text));
}
