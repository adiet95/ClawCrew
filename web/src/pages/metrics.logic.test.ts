import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { formatTokens, formatLatency, formatCostUsd } from './metrics.logic.js';

describe('metrics.logic', () => {
  describe('formatTokens', () => {
    it('formats millions', () => assert.equal(formatTokens(1_500_000), '1.5M'));
    it('formats thousands', () => assert.equal(formatTokens(1_500), '1.5K'));
    it('formats small numbers', () => assert.equal(formatTokens(42), '42'));
    it('formats exact million', () => assert.equal(formatTokens(1_000_000), '1.0M'));
  });

  describe('formatLatency', () => {
    it('formats null', () => assert.equal(formatLatency(null), '—'));
    it('formats ms', () => assert.equal(formatLatency(250), '250ms'));
    it('formats seconds', () => assert.equal(formatLatency(2500), '2.5s'));
    it('formats exact second', () => assert.equal(formatLatency(1000), '1.0s'));
  });

  describe('formatCostUsd', () => {
    it('formats small values', () => assert.equal(formatCostUsd(0.005), '<$0.01'));
    it('formats normal values', () => assert.equal(formatCostUsd(1.5), '$1.50'));
    it('formats zero', () => assert.equal(formatCostUsd(0), '<$0.01'));
    it('formats large values', () => assert.equal(formatCostUsd(123.456), '$123.46'));
  });
});
