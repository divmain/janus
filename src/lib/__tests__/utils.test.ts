import { describe, it } from 'node:test';
import assert from 'node:assert';

import { generateId, isoDate } from '../utils.ts';

describe('generateId', () => {
  it('generates an id with prefix and hash', () => {
    const id = generateId();
    assert.match(id, /^[a-z]+-[a-f0-9]{4}$/i);
  });

  it('generates unique ids', () => {
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      ids.add(generateId());
    }
    assert.strictEqual(ids.size, 100);
  });
});

describe('isoDate', () => {
  it('returns a valid ISO date string without milliseconds', () => {
    const date = isoDate();
    assert.match(date, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  });

  it('does not include milliseconds', () => {
    const date = isoDate();
    assert.ok(!date.includes('.'));
  });
});
