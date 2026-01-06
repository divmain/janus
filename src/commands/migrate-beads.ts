import fs from 'node:fs/promises';
import path from 'node:path';

import { TICKETS_DIR } from '../lib/types.ts';
import { ensureDir, isoDate } from '../lib/utils.ts';
import { exitWithError } from './utils.ts';

export async function cmdMigrateBeads(): Promise<void> {
  const jsonlPath = '.beads/issues.jsonl';

  try {
    await fs.access(jsonlPath);
  } catch {
    exitWithError(`Error: ${jsonlPath} not found`);
  }

  await ensureDir();

  const content = await fs.readFile(jsonlPath, 'utf-8');
  const lines = content.trim().split('\n');
  let migratedCount = 0;

  for (const line of lines) {
    if (!line.trim()) continue;

    const issue = JSON.parse(line);
    const id = issue.id;
    const now = isoDate();

    const deps = (issue.dependencies ?? [])
      .filter((d: { type: string }) => d?.type === 'blocks')
      .map((d: { depends_on_id: string }) => `"${d.depends_on_id}"`)
      .join(', ');

    const links = (issue.dependencies ?? [])
      .filter((d: { type: string }) => d?.type === 'related')
      .map((d: { depends_on_id: string }) => `"${d.depends_on_id}"`)
      .join(', ');

    const parentDep = (issue.dependencies ?? []).find(
      (d: { type: string }) => d?.type === 'parent-child'
    );
    const parent = parentDep?.depends_on_id ?? '';

    const frontmatter = [
      '---',
      `id: ${id}`,
      `status: ${issue.status ?? 'new'}`,
      `deps: [${deps}]`,
      `links: [${links}]`,
      `created: ${issue.created_at ?? now}`,
      `type: ${issue.issue_type ?? 'task'}`,
      `priority: ${issue.priority ?? 2}`,
      issue.assignee ? `assignee: ${issue.assignee}` : null,
      issue.external_ref ? `external-ref: ${issue.external_ref}` : null,
      parent ? `parent: ${parent}` : null,
      '---',
    ]
      .filter(Boolean)
      .join('\n');

    const sections = [
      `#${issue.title ?? 'Untitled'}`,
      issue.description ? `\n\n${issue.description}` : null,
      issue.design ? `\n\n## Design\n\n${issue.design}` : null,
      issue.acceptance_criteria ? `\n\n## Acceptance Criteria\n\n${issue.acceptance_criteria}` : null,
      issue.notes ? `\n\n## Notes\n\n${issue.notes}` : null,
    ]
      .filter(Boolean)
      .join('');

    const ticketContent = `${frontmatter}\n${sections}`;
    const filePath = path.join(TICKETS_DIR, `${id}.md`);

    await fs.writeFile(filePath, ticketContent, 'utf-8');
    console.log(`Migrated: ${id}`);
    migratedCount++;
  }

  console.log(`Migrated ${migratedCount} tickets from beads`);
}
