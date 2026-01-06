import { describe, it } from 'node:test';
import assert from 'node:assert';

import {
  Ok,
  Err,
  isOk,
  isErr,
  unwrap,
  unwrapOr,
  map,
  mapErr,
  tryCatch,
  tryCatchAsync,
} from '../result.ts';

describe('Result', () => {
  describe('Ok', () => {
    it('creates a successful result', () => {
      const result = Ok(42);
      assert.strictEqual(result.ok, true);
      assert.strictEqual(result.value, 42);
    });
  });

  describe('Err', () => {
    it('creates a failed result', () => {
      const result = Err('something went wrong');
      assert.strictEqual(result.ok, false);
      assert.strictEqual(result.error, 'something went wrong');
    });
  });

  describe('isOk', () => {
    it('returns true for Ok', () => {
      assert.strictEqual(isOk(Ok(1)), true);
    });

    it('returns false for Err', () => {
      assert.strictEqual(isOk(Err('error')), false);
    });
  });

  describe('isErr', () => {
    it('returns false for Ok', () => {
      assert.strictEqual(isErr(Ok(1)), false);
    });

    it('returns true for Err', () => {
      assert.strictEqual(isErr(Err('error')), true);
    });
  });

  describe('unwrap', () => {
    it('returns value for Ok', () => {
      assert.strictEqual(unwrap(Ok(42)), 42);
    });

    it('throws for Err with Error', () => {
      const error = new Error('test error');
      assert.throws(() => unwrap(Err(error)), /test error/);
    });

    it('throws for Err with string', () => {
      assert.throws(() => unwrap(Err('string error')), /string error/);
    });
  });

  describe('unwrapOr', () => {
    it('returns value for Ok', () => {
      assert.strictEqual(unwrapOr(Ok(42), 0), 42);
    });

    it('returns default for Err', () => {
      assert.strictEqual(unwrapOr(Err('error'), 0), 0);
    });
  });

  describe('map', () => {
    it('transforms Ok value', () => {
      const result = map(Ok(2), (x) => x * 3);
      assert.strictEqual(result.ok, true);
      assert.strictEqual((result as { ok: true; value: number }).value, 6);
    });

    it('passes through Err', () => {
      const result = map(Err('error'), (x: number) => x * 3);
      assert.strictEqual(result.ok, false);
      assert.strictEqual((result as { ok: false; error: string }).error, 'error');
    });
  });

  describe('mapErr', () => {
    it('passes through Ok', () => {
      const result = mapErr(Ok(42), (e: string) => `wrapped: ${e}`);
      assert.strictEqual(result.ok, true);
      assert.strictEqual((result as { ok: true; value: number }).value, 42);
    });

    it('transforms Err', () => {
      const result = mapErr(Err('error'), (e) => `wrapped: ${e}`);
      assert.strictEqual(result.ok, false);
      assert.strictEqual((result as { ok: false; error: string }).error, 'wrapped: error');
    });
  });

  describe('tryCatch', () => {
    it('returns Ok for successful function', () => {
      const result = tryCatch(() => 42);
      assert.strictEqual(result.ok, true);
      assert.strictEqual((result as { ok: true; value: number }).value, 42);
    });

    it('returns Err for throwing function', () => {
      const result = tryCatch(() => {
        throw new Error('boom');
      });
      assert.strictEqual(result.ok, false);
      assert.strictEqual((result as { ok: false; error: Error }).error.message, 'boom');
    });

    it('wraps non-Error throws', () => {
      const result = tryCatch(() => {
        throw 'string error';
      });
      assert.strictEqual(result.ok, false);
      assert.strictEqual((result as { ok: false; error: Error }).error.message, 'string error');
    });
  });

  describe('tryCatchAsync', () => {
    it('returns Ok for successful async function', async () => {
      const result = await tryCatchAsync(async () => 42);
      assert.strictEqual(result.ok, true);
      assert.strictEqual((result as { ok: true; value: number }).value, 42);
    });

    it('returns Err for rejecting async function', async () => {
      const result = await tryCatchAsync(async () => {
        throw new Error('async boom');
      });
      assert.strictEqual(result.ok, false);
      assert.strictEqual((result as { ok: false; error: Error }).error.message, 'async boom');
    });
  });
});
