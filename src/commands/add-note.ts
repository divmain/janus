import fs from 'node:fs/promises';
import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import { isoDate } from '../lib/utils.ts';
import { exitWithError, readStdin } from './utils.ts';

export async function cmdAddNote(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk add-note <id> [note text]');
  }

  const ticket = await Ticket.find(positionals[0]);
  let note = positionals.slice(1).join(' ') || '';

  if (!process.stdin.isTTY) {
    note = await readStdin();
  }

  let content = await fs.readFile(ticket.filePath, 'utf-8');

  if (!content.includes('## Notes')) {
    content += '\n## Notes';
  }

  const timestamp = isoDate();
  content += `\n\n**${timestamp}**\n\n${note}`;

  await fs.writeFile(ticket.filePath, content, 'utf-8');
  console.log(`Note added to ${ticket.id}`);
}
