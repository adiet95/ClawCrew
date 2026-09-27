import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { healthTone, healthLabel, isOnline } from './instances.logic.js';

describe('instances.logic', () => {
  describe('healthTone', () => {
    it('healthy → ok', () => assert.equal(healthTone('healthy'), 'ok'));
    it('degraded → warn', () => assert.equal(healthTone('degraded'), 'warn'));
    it('offline → error', () => assert.equal(healthTone('offline'), 'error'));
    it('unknown → neutral', () => assert.equal(healthTone('unknown'), 'neutral'));
  });

  describe('healthLabel', () => {
    it('capitalizes first letter', () => assert.equal(healthLabel('healthy'), 'Healthy'));
    it('capitalizes degraded', () => assert.equal(healthLabel('degraded'), 'Degraded'));
  });

  describe('isOnline', () => {
    it('healthy is online', () => assert.equal(isOnline('healthy'), true));
    it('degraded is online', () => assert.equal(isOnline('degraded'), true));
    it('offline is not online', () => assert.equal(isOnline('offline'), false));
    it('unknown is not online', () => assert.equal(isOnline('unknown'), false));
  });
});
