import { describe, it } from 'node:test';
import assert from 'node:assert';

import { parseTicketContent } from '../parser.ts';

describe('parseTicketContent', () => {
  it('parses basic ticket with frontmatter', () => {
    const content = `---
id: test-1234
status: new
deps: []
links: []
created: 2026-01-05T00:00:00Z
type: task
priority: 2
---
# Test Ticket

This is a description.
`;

    const result = parseTicketContent(content);

    assert.strictEqual(result.id, 'test-1234');
    assert.strictEqual(result.status, 'new');
    assert.strictEqual(result.type, 'task');
    assert.strictEqual(result.priority, '2');
    assert.strictEqual(result.title, 'Test Ticket');
    assert.deepStrictEqual(result.deps, []);
    assert.deepStrictEqual(result.links, []);
  });

  it('parses ticket with dependencies', () => {
    const content = `---
id: test-5678
status: new
deps: ["test-1234", "test-0000"]
links: ["test-9999"]
created: 2026-01-05T00:00:00Z
type: bug
priority: 1
---
# Bug Fix

Fix the issue.
`;

    const result = parseTicketContent(content);

    assert.strictEqual(result.id, 'test-5678');
    assert.strictEqual(result.type, 'bug');
    assert.strictEqual(result.priority, '1');
    assert.deepStrictEqual(result.deps, ['test-1234', 'test-0000']);
    assert.deepStrictEqual(result.links, ['test-9999']);
  });

  it('parses ticket with optional fields', () => {
    const content = `---
id: test-abcd
status: complete
deps: []
links: []
created: 2026-01-05T00:00:00Z
type: feature
priority: 0
assignee: John Doe
external-ref: gh-123
parent: test-0001
---
# Feature

A new feature.
`;

    const result = parseTicketContent(content);

    assert.strictEqual(result.assignee, 'John Doe');
    assert.strictEqual(result['external-ref'], 'gh-123');
    assert.strictEqual(result.parent, 'test-0001');
    assert.strictEqual(result.status, 'complete');
  });

  it('throws error for missing frontmatter', () => {
    const content = `# No Frontmatter

This ticket has no frontmatter.
`;

    assert.throws(
      () => parseTicketContent(content),
      /Invalid ticket format: missing YAML frontmatter/
    );
  });

  it('parses title from body correctly', () => {
    const content = `---
id: test-title
status: new
deps: []
links: []
created: 2026-01-05T00:00:00Z
type: task
priority: 2
---
# My Amazing Title

Some description here.

## Notes

More content.
`;

    const result = parseTicketContent(content);
    assert.strictEqual(result.title, 'My Amazing Title');
  });

  it('handles ticket with no title in body', () => {
    const content = `---
id: test-notitle
status: new
deps: []
links: []
created: 2026-01-05T00:00:00Z
type: task
priority: 2
---

Just description, no heading.
`;

    const result = parseTicketContent(content);
    assert.strictEqual(result.title, undefined);
  });
});
