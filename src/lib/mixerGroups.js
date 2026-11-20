export const GROUP_COUNT = 8;

export function defaultGroups() {
  return { faders: Array(GROUP_COUNT).fill(0), dca_muted: 0, mute_active: 0 };
}

export function defaultGroupLabels() {
  return { dca: Array(GROUP_COUNT).fill(''), mute: Array(GROUP_COUNT).fill('') };
}

export function hasGroup(mask, index) {
  return Boolean(mask & (1 << index));
}

function setGroupBit(mask, index, enabled) {
  const bit = 1 << index;
  return enabled ? mask | bit : mask & ~bit;
}

function mutedByGroup(groups, membership) {
  return Boolean((groups.dca_muted & membership.dca) || (groups.mute_active & membership.mute));
}

export function applyGroupChange(state, change) {
  const { type, index, enabled } = change;
  const groups = state.groups ?? defaultGroups();
  if (!Number.isInteger(index) || index < 0 || index >= GROUP_COUNT) throw new Error('Invalid group number');
  if (type === 'rename') {
    if (!['dca', 'mute'].includes(change.kind)) throw new Error('Invalid group kind');
    const name = change.name.trim();
    if (Array.from(name).length > 24 || /[\p{Cc}]/u.test(name)) throw new Error('Invalid group name');
    const labels = state.groupLabels ?? defaultGroupLabels();
    return { ...state, groupLabels: {
      ...labels, [change.kind]: labels[change.kind].map((label, i) => i === index ? name : label),
    } };
  }
  if (type === 'fader') {
    const db = Number.isFinite(change.db) ? Math.max(-90, Math.min(10, change.db)) : 0;
    return { ...state, groups: {
      ...groups, faders: groups.faders.map((value, group) => (group === index ? db : value)),
    } };
  }

  if (type === 'membership') {
    if (!['dca', 'mute'].includes(change.kind) || !state.strips.some((strip) => strip.index === change.strip)) {
      throw new Error('Invalid group assignment');
    }
    const strips = state.strips.map((strip) => {
      if (strip.index !== change.strip) return strip;
      const membership = strip.group_membership;
      const next = { ...membership, [change.kind]: setGroupBit(membership[change.kind], index, enabled) };
      return { ...strip, group_membership: next, group_muted: mutedByGroup(groups, next) };
    });
    return { ...state, strips };
  }

  const field = type === 'dcaMute' ? 'dca_muted' : type === 'muteGroup' ? 'mute_active' : null;
  if (!field) throw new Error('Invalid group control');
  const next = { ...groups, [field]: setGroupBit(groups[field], index, enabled) };
  return {
    ...state, groups: next,
    strips: state.strips.map((strip) => ({
      ...strip, group_muted: mutedByGroup(next, strip.group_membership),
    })),
  };
}

export function dcaDb(groups, membership) {
  const values = groups.faders.filter((_, i) => hasGroup(membership.dca, i));
  return values.some((db) => db <= -90) ? -Infinity : values.reduce((total, db) => total + db, 0);
}

export function groupLabel(labels, kind, index) {
  return labels?.[kind]?.[index] || `${kind === 'dca' ? 'DCA' : 'Mute group'} ${index + 1}`;
}
