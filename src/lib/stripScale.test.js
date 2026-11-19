import test from 'node:test';
import assert from 'node:assert/strict';
import { EXTRA_ROW_PX, FIT_MAX, FIT_MIN, STRIP_ROW_PX, stripRowPx, fitScale, nextStripSize, readStripSize, stripScale, stripSizeLabel } from './stripScale.js';

test('readStripSize falls back to fit', () => {
  assert.equal(readStripSize('1.25'), '1.25');
  assert.equal(readStripSize(null), 'fit');
  assert.equal(readStripSize('3'), 'fit');
});

test('nextStripSize cycles fit, 1, 1.25, 1.5', () => {
  assert.equal(nextStripSize('fit'), '1');
  assert.equal(nextStripSize('1'), '1.25');
  assert.equal(nextStripSize('1.25'), '1.5');
  assert.equal(nextStripSize('1.5'), 'fit');
});

test('fitScale shrinks no further than FIT_MIN, caps at the max and never overflows', () => {
  assert.equal(fitScale(0), 1);
  assert.equal(fitScale(NaN), 1);
  assert.equal(fitScale(100), FIT_MIN);
  assert.equal(fitScale(500), 0.8);
  assert.equal(fitScale(STRIP_ROW_PX), 1);
  assert.equal(fitScale(5000), FIT_MAX);
  for (const h of [480, 520, 600, 700, 777, 812, 900, 950]) {
    const s = fitScale(h);
    assert.ok(s * STRIP_ROW_PX <= h, `scale ${s} overflows ${h}`);
    assert.ok((s + 0.05) * STRIP_ROW_PX > h || s === FIT_MAX, `scale ${s} leaves room at ${h}`);
  }
});

test('stripScale uses the fixed size or fits', () => {
  assert.equal(stripScale('1.5', 100), 1.5);
  assert.equal(stripScale('fit', 885), 1.5);
  assert.equal(stripSizeLabel('fit'), 'size fit');
  assert.equal(stripSizeLabel('1.25'), 'size 1.25×');
});

test('stripRowPx adds the optional rows any strip shows', () => {
  assert.equal(stripRowPx([]), STRIP_ROW_PX);
  assert.equal(stripRowPx([{}]), STRIP_ROW_PX);
  const base = { automix: false, duck: false };
  assert.equal(stripRowPx([base]), STRIP_ROW_PX + EXTRA_ROW_PX.keys);
  assert.equal(stripRowPx([base, { ...base, automix: true }]), STRIP_ROW_PX + EXTRA_ROW_PX.keys + EXTRA_ROW_PX.readout);
  assert.equal(stripRowPx([{ ...base, duck: true }]), STRIP_ROW_PX + EXTRA_ROW_PX.keys + EXTRA_ROW_PX.knob + EXTRA_ROW_PX.readout);
});

test('fitScale uses the given row height', () => {
  assert.equal(fitScale(694, 694), 1);
  assert.ok(fitScale(600, 694) * 694 <= 600);
});
