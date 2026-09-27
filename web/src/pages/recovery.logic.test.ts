import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { recoveryTone, recoveryLabel, canRetry, canAcknowledge, filterByStatus } from './recovery.logic.js';

describe('recovery.logic', () => {
  describe('recoveryTone', () => {
    it('lost → error', () => assert.equal(recoveryTone('lost'), 'error'));
    it('failed → error', () => assert.equal(recoveryTone('failed'), 'error'));
    it('needs_review → warn', () => assert.equal(recoveryTone('needs_review'), 'warn'));
    it('timed_out → warn', () => assert.equal(recoveryTone('timed_out'), 'warn'));
    it('unknown → neutral', () => assert.equal(recoveryTone('other'), 'neutral'));
  });

  describe('recoveryLabel', () => {
    it('replaces underscores', () => assert.equal(recoveryLabel('needs_review'), 'needs review'));
    it('handles simple', () => assert.equal(recoveryLabel('lost'), 'lost'));
  });

  describe('canRetry', () => {
    it('failed can retry', () => assert.ok(canRetry('failed')));
    it('needs_review can retry', () => assert.ok(canRetry('needs_review')));
    it('lost cannot retry', () => assert.ok(!canRetry('lost')));
  });

  describe('canAcknowledge', () => {
    it('lost can acknowledge', () => assert.ok(canAcknowledge('lost')));
    it('timed_out can acknowledge', () => assert.ok(canAcknowledge('timed_out')));
    it('failed cannot acknowledge', () => assert.ok(!canAcknowledge('failed')));
  });

  describe('filterByStatus', () => {
    const tasks = [
      { status: 'lost', id: '1' },
      { status: 'failed', id: '2' },
      { status: 'lost', id: '3' },
    ];
    it('returns all when filter is null', () => assert.equal(filterByStatus(tasks, null).length, 3));
    it('filters by status', () => assert.equal(filterByStatus(tasks, 'lost').length, 2));
    it('returns empty for no match', () => assert.equal(filterByStatus(tasks, 'timed_out').length, 0));
  });
});
