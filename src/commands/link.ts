import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import { exitWithError } from './utils.ts';

export async function cmdLink(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 2) {
    exitWithError('Usage: tk link <id> <id> [id...]');
  }

  const tickets = await Promise.all(positionals.map((id) => Ticket.find(id)));
  let addedCount = 0;

  for (const ticket of tickets) {
    for (const other of tickets) {
      if (ticket.id !== other.id) {
        const added = await ticket.addToArrayField('links', other.id);
        if (added) addedCount++;
      }
    }
  }

  console.log(
    addedCount === 0
      ? 'All links already exist'
      : `Added ${addedCount} link(s) between ${tickets.length} tickets`
  );
}

export async function cmdUnlink(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 2) {
    exitWithError('Usage: tk unlink <id> <target-id>');
  }

  const [id, targetId] = positionals;
  const ticket1 = await Ticket.find(id);
  const ticket2 = await Ticket.find(targetId);

  let removedCount = 0;
  if (await ticket1.removeFromArrayField('links', targetId)) removedCount++;
  if (await ticket2.removeFromArrayField('links', id)) removedCount++;

  if (removedCount === 0) {
    exitWithError('Link not found');
  }

  console.log(`Removed link: ${ticket1.id} <-> ${targetId}`);
}
