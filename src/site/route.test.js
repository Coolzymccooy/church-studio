import test from 'node:test';
import assert from 'node:assert/strict';
import { wantsStudio } from './route.js';

const at = (pathname = '/', search = '', hash = '') => ({ pathname, search, hash });

test('the root shows the site', () => {
  assert.equal(wantsStudio(at('/')), false);
  assert.equal(wantsStudio(at('/', '?kit')), false);
  assert.equal(wantsStudio(at('/', '?apple=1')), false);
  assert.equal(wantsStudio(at('/studios')), false);
});

test('old ?app and #app links still open the studio', () => {
  assert.equal(wantsStudio(at('/', '?app')), true);
  assert.equal(wantsStudio(at('/', '?app=1')), true);
  assert.equal(wantsStudio(at('/', '?x=1&app')), true);
  assert.equal(wantsStudio(at('/', '', '#app')), true);
});

test('/studio opens the studio', () => {
  assert.equal(wantsStudio(at('/studio')), true);
  assert.equal(wantsStudio(at('/studio/')), true);
  assert.equal(wantsStudio(at('/studio/index.html')), true);
});

test('a missing location shows the site', () => {
  assert.equal(wantsStudio(undefined), false);
  assert.equal(wantsStudio({}), false);
});
