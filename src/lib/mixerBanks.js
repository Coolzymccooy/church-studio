/** Strip banks for the mixer: eight input strips per bank, then NDI strips, plus "all". */
export const BANK_SIZE = 8;

/**
 * Banks for `stripCount` strips of which the last `ndiCount` are NDI. Only "all" when everything
 * fits in one bank. Each bank is { id, label, from, to } over strip indices (to exclusive).
 */
export function stripBanks(stripCount, ndiCount = 0) {
  const total = Math.max(0, stripCount | 0);
  const ndi = Math.min(total, Math.max(0, ndiCount | 0));
  const inputs = total - ndi;
  const all = { id: 'all', label: 'all', from: 0, to: total };
  if (total <= BANK_SIZE) return [all];
  const banks = [all];
  for (let from = 0; from < inputs; from += BANK_SIZE) {
    const to = Math.min(inputs, from + BANK_SIZE);
    banks.push({ id: `in-${from}`, label: `${from + 1}–${to}`, from, to });
  }
  if (ndi > 0) banks.push({ id: 'ndi', label: 'ndi', from: inputs, to: total });
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
