export function changeBody(layout, target, kind, value) {
  return {
    layout,
    target,
    kind,
    ...(kind === 'mute' ? { muted: Boolean(value) } : { db: Number(value) }),
  };
}

export function dbLabel(db) {
  return db <= -90 ? 'Off' : `${db.toFixed(1)} dB`;
}

export function pairUrl(address, code) {
  const url = new URL('/', address);
  url.hash = new URLSearchParams({ code }).toString();
  return url.href;
}

export function pairCodeFromHash(hash) {
  const code = new URLSearchParams(hash.slice(1)).get('code');
  return /^\d{6}$/.test(code ?? '') ? code : '';
}
