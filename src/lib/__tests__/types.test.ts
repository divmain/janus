import { describe, it } from 'node:test';
import assert from 'node:assert';

import { isValidStatus, isValidType, isValidPriority } from '../types.ts';

describe('isValidStatus', () => {
  it('returns true for valid statuses', () => {
    assert.strictEqual(isValidStatus('new'), true);
    assert.strictEqual(isValidStatus('cancelled'), true);
    assert.strictEqual(isValidStatus('complete'), true);
  });

  it('returns false for invalid statuses', () => {
    assert.strictEqual(isValidStatus('pending'), false);
    assert.strictEqual(isValidStatus('in-progress'), false);
    assert.strictEqual(isValidStatus('done'), false);
    assert.strictEqual(isValidStatus(''), false);
  });
});

describe('isValidType', () => {
  it('returns true for valid types', () => {
    assert.strictEqual(isValidType('bug'), true);
    assert.strictEqual(isValidType('feature'), true);
    assert.strictEqual(isValidType('task'), true);
    assert.strictEqual(isValidType('epic'), true);
    assert.strictEqual(isValidType('chore'), true);
  });

  it('returns false for invalid types', () => {
    assert.strictEqual(isValidType('story'), false);
    assert.strictEqual(isValidType('improvement'), false);
    assert.strictEqual(isValidType(''), false);
  });
});

describe('isValidPriority', () => {
  it('returns true for valid priorities', () => {
    assert.strictEqual(isValidPriority('0'), true);
    assert.strictEqual(isValidPriority('1'), true);
    assert.strictEqual(isValidPriority('2'), true);
    assert.strictEqual(isValidPriority('3'), true);
    assert.strictEqual(isValidPriority('4'), true);
  });

  it('returns false for invalid priorities', () => {
    assert.strictEqual(isValidPriority('5'), false);
    assert.strictEqual(isValidPriority('-1'), false);
    assert.strictEqual(isValidPriority('high'), false);
    assert.strictEqual(isValidPriority(''), false);
  });
});
