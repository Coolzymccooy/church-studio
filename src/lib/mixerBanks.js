/** Strip banks for the mixer: eight input strips per bank, then NDI strips, plus "all". */
export const BANK_SIZE = 8;

/**
 * Strip counts from a mixer state. The engine serialises the top level in camelCase
 * (`ndiStrips`, `computerStrips`); snake_case is accepted too for older or demo states.
 */
export function stripCounts(state) {
  const n = (v) => (Number.isFinite(v) && v > 0 ? Math.floor(v) : 0);
  return {
    total: Array.isArray(state?.strips) ? state.strips.length : 0,
    ndi: n(state?.ndiStrips ?? state?.ndi_strips),
    computer: n(state?.computerStrips ?? state?.computer_strips),
  };
}

/**
 * Banks for `total` strips: hardware inputs first, then the computer-audio pair, then NDI (the
 * engine's strip order). Only "all" when everything fits in one bank. Each bank is
 * { id, label, from, to } over strip indices (to exclusive).
 */
export function stripBanks(total, ndiCount = 0, computerCount = 0) {
  const all = Math.max(0, total | 0);
  const ndi = Math.min(all, Math.max(0, ndiCount | 0));
  const pc = Math.min(all - ndi, Math.max(0, computerCount | 0));
  const inputs = all - ndi - pc;
  const banks = [{ id: 'all', label: 'all', from: 0, to: all }];
  if (all <= BANK_SIZE) return banks;
  for (let from = 0; from < inputs; from += BANK_SIZE) {
    const to = Math.min(inputs, from + BANK_SIZE);
    banks.push({ id: `in-${from}`, label: `${from + 1}–${to}`, from, to });
  }
  if (pc > 0) banks.push({ id: 'pc', label: 'computer', from: inputs, to: inputs + pc });
  if (ndi > 0) banks.push({ id: 'ndi', label: 'ndi', from: all - ndi, to: all });
  return banks;
}

/** The bank with `id`, or "all" when it no longer exists (strip count changed). */
export function pickBank(banks, id) {
  return banks.find((b) => b.id === id) ?? banks[0];
}

/** Strips inside a bank, by index. */
export function bankStrips(strips, bank) {
  return strips.filter((s) => s.index >= bank.from && s.index < bank.to);
}

/** Page `page` (0-based) of `size` items, with whether earlier or later pages exist. */
export function pageOf(items, page, size) {
  const pages = Math.max(1, Math.ceil(items.length / size));
  const p = Math.min(Math.max(0, page), pages - 1);
  return { items: items.slice(p * size, p * size + size), page: p, hasPrev: p > 0, hasNext: p < pages - 1, start: p * size };
}
