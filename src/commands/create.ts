import fs from 'node:fs/promises';
import path from 'node:path';
import { parseArgs, type ParseArgsConfig } from 'node:util';

import { TICKETS_DIR } from '../lib/types.ts';
import { ensureDir, generateId, getGitUserName, isoDate } from '../lib/utils.ts';

export async function cmdCreate(args: string[]): Promise<void> {
  const options: ParseArgsConfig['options'] = {
    description: { type: 'string', short: 'd', default: '' },
    design: { type: 'string', default: '' },
    acceptance: { type: 'string', default: '' },
    priority: { type: 'string', short: 'p', default: '2' },
    type: { type: 'string', short: 't', default: 'task' },
    assignee: { type: 'string', short: 'a' },
    'external-ref': { type: 'string', default: '' },
    parent: { type: 'string', default: '' },
  };

  const { values, positionals } = parseArgs({
    args,
    options,
    allowPositionals: true,
  });

  await ensureDir();

  const title = positionals[0] || 'Untitled';
  const assignee = values.assignee ?? (await getGitUserName());

  const id = generateId();
  const now = isoDate();

  const frontmatter = [
    '---',
    `id: ${id}`,
    `status: new`,
    `deps: []`,
    `links: []`,
    `created: ${now}`,
    `type: ${values.type}`,
    `priority: ${values.priority}`,
    assignee ? `assignee: ${assignee}` : null,
    values['external-ref'] ? `external-ref: ${values['external-ref']}` : null,
    values.parent ? `parent: ${values.parent}` : null,
    '---',
  ]
    .filter(Boolean)
    .join('\n');

  const sections = [
    `# ${title}`,
    values.description ? `\n${values.description}` : null,
    values.design ? `\n## Design\n\n${values.design}` : null,
    values.acceptance ? `\n## Acceptance Criteria\n\n${values.acceptance}` : null,
  ]
    .filter(Boolean)
    .join('\n');

  const content = `${frontmatter}\n${sections}\n`;
  const file = path.join(TICKETS_DIR, `${id}.md`);

  await fs.mkdir(TICKETS_DIR, { recursive: true });
  await fs.writeFile(file, content, 'utf-8');

  console.log(id);
}
