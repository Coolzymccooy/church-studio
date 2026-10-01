import test from 'node:test';
import assert from 'node:assert/strict';

import {
  DEFAULT_NDI_BASE_NAME,
  MAX_NDI_BASE_NAME_CHARS,
  buildNdiOutputsArgs,
  createNdiClient,
  describeNdiStatus,
  ndiSourceName,
  normalizeNdiOutputs,
  sanitizeNdiBaseName,
  sendingFromEngineStatus,
} from './ndiOutput.js';

test('sanitizeNdiBaseName trims and collapses whitespace', () => {
  assert.equal(sanitizeNdiBaseName('  Grace   Church \n Live '), 'Grace Church Live');
});

test('sanitizeNdiBaseName strips forbidden and control characters', () => {
  assert.equal(sanitizeNdiBaseName('A/B\\C:(D)*?"<>|\u0007E'), 'ABCDE');
});

test('sanitizeNdiBaseName falls back to the default when empty', () => {
  assert.equal(sanitizeNdiBaseName(''), DEFAULT_NDI_BASE_NAME);
  assert.equal(sanitizeNdiBaseName('   '), DEFAULT_NDI_BASE_NAME);
  assert.equal(sanitizeNdiBaseName('()'), DEFAULT_NDI_BASE_NAME);
  assert.equal(sanitizeNdiBaseName(undefined), DEFAULT_NDI_BASE_NAME);
});

test('sanitizeNdiBaseName caps at 60 characters (code points)', () => {
  assert.equal(Array.from(sanitizeNdiBaseName('é'.repeat(80))).length, MAX_NDI_BASE_NAME_CHARS);
  assert.equal(sanitizeNdiBaseName(`${'x'.repeat(59)} tail`), 'x'.repeat(59));
});

test('ndiSourceName appends the bus in parentheses', () => {
  assert.equal(ndiSourceName('', 'Stream'), 'TIWATON Studio (Stream)');
  assert.equal(ndiSourceName(' Grace ', 'Main'), 'Grace (Main)');
});

test('buildNdiOutputsArgs matches the ndi_set_outputs contract', () => {
  assert.deepEqual(
    buildNdiOutputsArgs({ stream: 1, main: false, baseName: '  Grace  ' }),
    { outputs: { stream: true, main: false, monitor: false, base_name: 'Grace' } },
  );
  assert.deepEqual(buildNdiOutputsArgs(), {
    outputs: { stream: false, main: false, monitor: false, base_name: DEFAULT_NDI_BASE_NAME },
  });
});

test('normalizeNdiOutputs maps the backend shape', () => {
  assert.deepEqual(normalizeNdiOutputs({ stream: true, base_name: 'Grace' }), {
    stream: true,
    main: false,
    monitor: false,
    baseName: 'Grace',
  });
  assert.equal(normalizeNdiOutputs(null).baseName, DEFAULT_NDI_BASE_NAME);
});

test('describeNdiStatus covers checking, installed and missing', () => {
  assert.equal(describeNdiStatus(null).available, false);
  assert.equal(describeNdiStatus({ available: false, loading: true }).label, 'Checking NDI® runtime…');
  assert.deepEqual(describeNdiStatus({ available: true, version: '6.1.0' }), {
    available: true,
    label: 'NDI® runtime 6.1.0',
  });
  assert.equal(describeNdiStatus({ available: false }).label, 'Not installed — get NDI Tools');
});

test('sendingFromEngineStatus reads engine_status.ndi.sending', () => {
  assert.deepEqual(sendingFromEngineStatus({ ndi: { sending: ['A (Stream)', 3] } }), ['A (Stream)']);
  assert.deepEqual(sendingFromEngineStatus({ running: false }), []);
  assert.deepEqual(sendingFromEngineStatus(null), []);
});

test('createNdiClient calls the Tauri commands with the right arguments', async () => {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push([command, args]);
    if (command === 'ndi_set_outputs') {
      return { outputs: args.outputs, running: true, restart_required: true, message: 'Restart', save_error: null };
    }
    if (command === 'engine_status') return { ndi: { sending: ['X (Main)'] } };
    if (command === 'ndi_get_outputs') return { main: true, base_name: 'X' };
    return { available: false };
  };
  const client = createNdiClient(invoke);

  const result = await client.setOutputs({ main: true, baseName: 'X' });
  assert.deepEqual(calls[0], [
    'ndi_set_outputs',
    { outputs: { stream: false, main: true, monitor: false, base_name: 'X' } },
  ]);
  assert.equal(result.restartRequired, true);
  assert.equal(result.outputs.main, true);
  assert.equal(result.message, 'Restart');
  assert.equal(result.saveError, null);

  assert.deepEqual(await client.getSending(), ['X (Main)']);
  assert.equal((await client.getOutputs()).baseName, 'X');
  assert.deepEqual(await client.getStatus(), { available: false });
});
