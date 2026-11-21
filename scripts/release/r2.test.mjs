import assert from 'node:assert/strict';
import test from 'node:test';
import { r2Settings } from './r2.mjs';

const GOOD = { R2_ACCOUNT_ID: 'a'.repeat(32), R2_ACCESS_KEY_ID: 'AKIDEXAMPLE', R2_SECRET_ACCESS_KEY: 'secretsecret', PATH: '/bin' };

test('R2 settings build the account endpoint and a clean child env', () => {
  const settings = r2Settings(GOOD);
  assert.deepEqual(settings.problems, []);
  assert.equal(settings.endpoint, `https://${'a'.repeat(32)}.r2.cloudflarestorage.com`);
  assert.equal(settings.childEnv.AWS_ACCESS_KEY_ID, 'AKIDEXAMPLE');
  assert.equal(settings.childEnv.AWS_REGION, 'auto');
  assert.equal(settings.childEnv.AWS_REQUEST_CHECKSUM_CALCULATION, 'when_required');
  assert.equal(settings.childEnv.AWS_PROFILE, undefined);
});

test('R2 settings name every missing key and reject a bad account id', () => {
  const settings = r2Settings({ R2_ACCOUNT_ID: 'not-an-id' });
  assert.equal(settings.problems.length, 3);
  assert.match(settings.problems.join('\n'), /R2_ACCESS_KEY_ID/);
  assert.match(settings.problems.join('\n'), /32-character/);
});
