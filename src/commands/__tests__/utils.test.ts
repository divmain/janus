import { describe, it } from 'node:test';
import assert from 'node:assert';

import type { TicketMetadata } from '../../lib/types.ts';
import { formatDeps, formatTicketBullet, formatTicketLine, sortByPriority } from '../utils.ts';

describe('sortByPriority', () => {
  it('sorts tickets by priority ascending', () => {
    const tickets: TicketMetadata[] = [
      { id: 'a', priority: '3' },
      { id: 'b', priority: '1' },
      { id: 'c', priority: '2' },
    ];

    sortByPriority(tickets);

    assert.strictEqual(tickets[0].id, 'b');
    assert.strictEqual(tickets[1].id, 'c');
    assert.strictEqual(tickets[2].id, 'a');
  });

  it('sorts by id when priorities are equal', () => {
    const tickets: TicketMetadata[] = [
      { id: 'z-1', priority: '2' },
      { id: 'a-1', priority: '2' },
      { id: 'm-1', priority: '2' },
    ];

    sortByPriority(tickets);

    assert.strictEqual(tickets[0].id, 'a-1');
    assert.strictEqual(tickets[1].id, 'm-1');
    assert.strictEqual(tickets[2].id, 'z-1');
  });

  it('defaults to priority 2 when not specified', () => {
    const tickets: TicketMetadata[] = [
      { id: 'a', priority: '1' },
      { id: 'b' }, // no priority, defaults to 2
      { id: 'c', priority: '3' },
    ];

    sortByPriority(tickets);

    assert.strictEqual(tickets[0].id, 'a');
    assert.strictEqual(tickets[1].id, 'b');
    assert.strictEqual(tickets[2].id, 'c');
  });
});

describe('formatTicketLine', () => {
  it('formats a basic ticket line', () => {
    const ticket: TicketMetadata = {
      id: 'test-1',
      status: 'new',
      title: 'Test Ticket',
    };

    const result = formatTicketLine(ticket);

    assert.strictEqual(result, 'test-1   [new] - Test Ticket');
  });

  it('formats a ticket with priority', () => {
    const ticket: TicketMetadata = {
      id: 'test-2',
      status: 'complete',
      title: 'Done Ticket',
      priority: '1',
    };

    const result = formatTicketLine(ticket, { showPriority: true });

    assert.strictEqual(result, 'test-2   [P1][complete] - Done Ticket');
  });

  it('formats a ticket with suffix', () => {
    const ticket: TicketMetadata = {
      id: 'test-3',
      status: 'new',
      title: 'Blocked Ticket',
    };

    const result = formatTicketLine(ticket, { suffix: ' <- [dep-1]' });

    assert.strictEqual(result, 'test-3   [new] - Blocked Ticket <- [dep-1]');
  });

  it('handles missing id and title', () => {
    const ticket: TicketMetadata = {
      status: 'new',
    };

    const result = formatTicketLine(ticket);

    assert.strictEqual(result, '???      [new] - ');
  });
});

describe('formatDeps', () => {
  it('formats a list of dependencies', () => {
    const deps = ['dep-1', 'dep-2', 'dep-3'];

    const result = formatDeps(deps);

    assert.strictEqual(result, ' <- [dep-1, dep-2, dep-3]');
  });

  it('formats an empty list', () => {
    const result = formatDeps([]);

    assert.strictEqual(result, ' <- []');
  });
});

describe('formatTicketBullet', () => {
  it('formats a ticket as a bullet point', () => {
    const ticket: TicketMetadata = {
      id: 'test-1',
      status: 'new',
      title: 'Test Ticket',
    };

    const result = formatTicketBullet(ticket);

    assert.strictEqual(result, '- test-1 [new] Test Ticket');
  });
});
