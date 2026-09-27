import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { appStateTone, canRemoveApp, verdictTone } from './apps.logic.js';

describe('apps.logic', () => {
  describe('appStateTone', () => {
    it('enabled → ok', () => assert.equal(appStateTone('enabled'), 'ok'));
    it('disabled → warn', () => assert.equal(appStateTone('disabled'), 'warn'));
    it('installed → neutral', () => assert.equal(appStateTone('installed'), 'neutral'));
  });

  describe('canRemoveApp', () => {
    it('cannot remove enabled app', () => assert.equal(canRemoveApp('enabled'), false));
    it('can remove disabled app', () => assert.equal(canRemoveApp('disabled'), true));
    it('can remove installed app', () => assert.equal(canRemoveApp('installed'), true));
  });

  describe('verdictTone', () => {
    it('allow → ok', () => assert.equal(verdictTone('allow'), 'ok'));
    it('deny → error', () => assert.equal(verdictTone('deny'), 'error'));
    it('ask → warn', () => assert.equal(verdictTone('ask'), 'warn'));
  });
});
