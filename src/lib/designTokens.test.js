import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const CSS = readFileSync(new URL('../styles/faceplate-tokens.css', import.meta.url), 'utf8');

const blockAfter = (marker) => {
  const start = CSS.indexOf(marker);
  assert.notEqual(start, -1, `missing block: ${marker}`);
  const open = CSS.indexOf('{', start + marker.length - 1);
  let depth = 0;
  for (let i = open; i < CSS.length; i += 1) {
    if (CSS[i] === '{') depth += 1;
    if (CSS[i] === '}') { depth -= 1; if (depth === 0) return CSS.slice(open + 1, i); }
  }
  throw new Error(`unclosed block: ${marker}`);
};

const tokensIn = (body) => Object.fromEntries(
  [...body.matchAll(/(--fp-[a-z0-9-]+):\s*([^;]+);/g)].map(([, k, v]) => [k, v.trim()]),
);

const shared = tokensIn(blockAfter('/* ---------- shared'));
const graphite = tokensIn(blockAfter('/* ---------- finish: graphite'));
const bone = tokensIn(blockAfter(':root[data-finish="bone"]'));
const auto = tokensIn(blockAfter(':root[data-finish="auto"]'));

const finish = (name) => ({ ...shared, ...(name === 'bone' ? bone : graphite) });

const luminance = (hex) => {
  const ch = hex.replace('#', '').match(/../g).map((h) => parseInt(h, 16) / 255)
    .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
};
const contrast = (a, b) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

// [foreground, background] pairs that carry text in the faceplate designs. AA for small text: 4.5.
const TEXT_PAIRS = [
  ['--fp-legend', '--fp-chassis'], ['--fp-legend', '--fp-plate'], ['--fp-legend', '--fp-raised'],
  ['--fp-muted', '--fp-chassis'], ['--fp-muted', '--fp-plate'], ['--fp-muted', '--fp-raised'],
  ['--fp-key-text', '--fp-key'], ['--fp-sel-ink', '--fp-sel'],
  ['--fp-arm', '--fp-arm-bg'], ['--fp-arm-r', '--fp-arm-r-bg'],
  ['--fp-bus-line', '--fp-bus-cap'],
  ['--fp-glass-text', '--fp-glass'], ['--fp-glass-dim', '--fp-glass'],
  ['--fp-led-g', '--fp-glass'], ['--fp-led-a', '--fp-glass'], ['--fp-led-r', '--fp-glass'],
  ['--fp-ink', '--fp-led-a'], ['--fp-ink', '--fp-orange'],
  ['--fp-on-green', '--fp-led-g'], ['--fp-on-red', '--fp-led-r'],
  ['--fp-ink', '--fp-ch-amber'], ['--fp-ink', '--fp-ch-clay'], ['--fp-ink', '--fp-ch-teal'],
  ['--fp-ink', '--fp-ch-olive'], ['--fp-ink', '--fp-ch-sky'], ['--fp-ink', '--fp-ch-rose'],
  ['--fp-ink', '--fp-ch-stone'],
];

// Boundaries an operator must see (focus ring, key outline). AA for UI parts: 3.
const UI_PAIRS = [
  ['--fp-focus', '--fp-chassis'], ['--fp-focus', '--fp-plate'], ['--fp-focus', '--fp-raised'],
  ['--fp-key-edge', '--fp-plate'], ['--fp-key-edge', '--fp-raised'],
];

test('graphite and bone define the same finish tokens', () => {
  assert.deepEqual(Object.keys(bone).sort(), Object.keys(graphite).sort());
});

test('auto finish matches bone exactly', () => {
  assert.deepEqual(auto, bone);
});

test('finish tokens never redefine shared tokens', () => {
  const clash = Object.keys(graphite).filter((k) => k in shared);
  assert.deepEqual(clash, []);
});

test('every colour token is a six-digit hex', () => {
  for (const name of ['graphite', 'bone']) {
    for (const [k, v] of Object.entries(finish(name))) {
      if (/^--fp-(font|text|weight|radius|inset|elev|glow|motion|arm-timeout)/.test(k)) continue;
      assert.match(v, /^#[0-9A-F]{6}$/i, `${name} ${k} = ${v}`);
    }
  }
});

test('type scale never goes under the 11px floor', () => {
  for (const [k, v] of Object.entries(shared).filter(([key]) => key.startsWith('--fp-text-'))) {
    assert.ok(parseInt(v, 10) >= 11, `${k} = ${v}`);
  }
});

for (const name of ['graphite', 'bone']) {
  test(`${name}: text pairs pass WCAG AA (4.5:1)`, () => {
    const t = finish(name);
    const fails = TEXT_PAIRS
      .map(([fg, bg]) => [fg, bg, contrast(t[fg], t[bg])])
      .filter(([, , ratio]) => ratio < 4.5)
      .map(([fg, bg, ratio]) => `${fg} on ${bg}: ${ratio.toFixed(2)}`);
    assert.deepEqual(fails, []);
  });

  test(`${name}: focus ring and key outlines pass WCAG AA (3:1)`, () => {
    const t = finish(name);
    const fails = UI_PAIRS
      .map(([fg, bg]) => [fg, bg, contrast(t[fg], t[bg])])
      .filter(([, , ratio]) => ratio < 3)
      .map(([fg, bg, ratio]) => `${fg} on ${bg}: ${ratio.toFixed(2)}`);
    assert.deepEqual(fails, []);
  });
}

test('every colour token has a Tailwind fp- utility', () => {
  const theme = blockAfter('@theme inline');
  const missing = Object.keys({ ...shared, ...graphite })
    .filter((k) => /^#/.test(finish('graphite')[k]))
    .filter((k) => !theme.includes(`--color-${k.slice(2)}: var(${k});`));
  assert.deepEqual(missing, []);
});

test('index.css keeps every @import above all other statements', () => {
  // CSS ignores an @import that follows any other rule, which silently dropped the faceplate
  // tokens once (PR 31). Strip comments, then every @import must come before the first non-import.
  const indexCss = readFileSync(new URL('../index.css', import.meta.url), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '');
  const statements = indexCss.split(';').map((x) => x.trim()).filter(Boolean);
  const firstOther = statements.findIndex((x) => !x.startsWith('@import'));
  const lateImports = statements.slice(firstOther).filter((x) => x.startsWith('@import'));
  assert.deepEqual(lateImports, []);
  assert.ok(statements.slice(0, firstOther).some((x) => x.includes('faceplate-tokens.css')));
  assert.ok(statements.slice(0, firstOther).some((x) => x.includes('faceplate-parts.css')));
});
