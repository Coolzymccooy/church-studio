import test from 'node:test';
import assert from 'node:assert/strict';

import {
  MAX_NDI_INPUTS,
  MAX_NDI_SOURCE_NAME_CHARS,
  buildNdiInputsArgs,
  createNdiInputClient,
  inputsFromEngineStatus,
  mergeSourceList,
  normalizeNdiInputs,
  normalizeNdiSources,
  sanitizeNdiSourceList,
  toggleNdiSource,
} from './ndiInput.js';

test('sanitizeNdiSourceList trims, dedupes and caps at four', () => {
  assert.deepEqual(
    sanitizeNdiSourceList([' A (One) ', '', 'A (One)', 'B\u0007 (x)', 'C', 'D', 'E', 'F', 42]),
    ['A (One)', 'C', 'D', 'E'],
  );
  assert.deepEqual(sanitizeNdiSourceList(null), []);
});

test('sanitizeNdiSourceList drops overlong names', () => {
  const ok = 'y'.repeat(MAX_NDI_SOURCE_NAME_CHARS);
  assert.deepEqual(sanitizeNdiSourceList(['x'.repeat(MAX_NDI_SOURCE_NAME_CHARS + 1), ok]), [ok]);
});

test('toggleNdiSource adds, removes and respects the limit', () => {
  assert.deepEqual(toggleNdiSource([], 'A', true), ['A']);
  assert.deepEqual(toggleNdiSource(['A', 'B'], 'A', false), ['B']);
  assert.deepEqual(toggleNdiSource(['A'], 'A', true), ['A']);
  const full = ['A', 'B', 'C', 'D'];
  assert.equal(full.length, MAX_NDI_INPUTS);
  assert.deepEqual(toggleNdiSource(full, 'E', true), full);
});

test('buildNdiInputsArgs matches the ndi_set_inputs contract', () => {
  assert.deepEqual(buildNdiInputsArgs([' PC (Keys) ', 'PC (Keys)']), {
    inputs: { sources: ['PC (Keys)'] },
  });
  assert.deepEqual(buildNdiInputsArgs(), { inputs: { sources: [] } });
});

test('normalizeNdiInputs tolerates missing fields', () => {
  assert.deepEqual(normalizeNdiInputs({ sources: ['PC (Keys)'] }), { sources: ['PC (Keys)'] });
  assert.deepEqual(normalizeNdiInputs(null), { sources: [] });
});

test('normalizeNdiSources maps the ndi_list_sources result', () => {
  assert.deepEqual(
    normalizeNdiSources([
      { name: 'PC (Keys)', url: '10.0.0.2:5961', own: false },
      { name: 'PC (TIWATON Studio (Stream))', own: true },
      { name: '' },
      'junk',
    ]),
    [
      { name: 'PC (Keys)', url: '10.0.0.2:5961', own: false },
      { name: 'PC (TIWATON Studio (Stream))', url: null, own: true },
    ],
  );
  assert.deepEqual(normalizeNdiSources(undefined), []);
});

test('mergeSourceList keeps selected sources that are offline', () => {
  const found = [{ name: 'PC (Keys)', url: null, own: false }];
  assert.deepEqual(mergeSourceList(found, ['PC (Video)', 'PC (Keys)']), [
    { name: 'PC (Keys)', url: null, own: false, offline: false },
    { name: 'PC (Video)', url: null, own: false, offline: true },
  ]);
});

test('inputsFromEngineStatus reads engine_status.ndi.inputs', () => {
  const status = {
    ndi: {
      inputs: [
        { source: 'PC (Keys)', strip: 2, connected: true, fill_ms: 41, dropped_samples: 0, underruns: 1 },
        { strip: 3 },
      ],
    },
  };
  assert.deepEqual(inputsFromEngineStatus(status), [
    { source: 'PC (Keys)', strip: 2, connected: true, fillMs: 41, droppedSamples: 0, underruns: 1 },
  ]);
  assert.deepEqual(inputsFromEngineStatus({ running: false }), []);
  assert.deepEqual(inputsFromEngineStatus(null), []);
});

test('createNdiInputClient calls the Tauri commands with the right arguments', async () => {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push([command, args]);
    if (command === 'ndi_set_inputs') {
      return { inputs: args.inputs, running: true, restart_required: true, message: 'Restart', save_error: null };
    }
    if (command === 'ndi_list_sources') return [{ name: 'PC (Keys)' }];
    if (command === 'ndi_get_inputs') return { sources: ['PC (Keys)'] };
    if (command === 'engine_status') return { ndi: { inputs: [{ source: 'PC (Keys)', strip: 1 }] } };
    return null;
  };
  const client = createNdiInputClient(invoke);

  const result = await client.setInputs(['PC (Keys)']);
  assert.deepEqual(calls[0], ['ndi_set_inputs', { inputs: { sources: ['PC (Keys)'] } }]);
  assert.equal(result.restartRequired, true);
  assert.deepEqual(result.inputs, { sources: ['PC (Keys)'] });
  assert.equal(result.message, 'Restart');
  assert.equal(result.saveError, null);

  assert.deepEqual(await client.listSources(), [{ name: 'PC (Keys)', url: null, own: false }]);
  assert.deepEqual(await client.getInputs(), { sources: ['PC (Keys)'] });
  const live = await client.getInputStatus();
  assert.equal(live[0].strip, 1);
  assert.equal(live[0].connected, false);
});
