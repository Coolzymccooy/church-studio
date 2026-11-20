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
