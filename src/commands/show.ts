import fs from 'node:fs/promises';
import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import type { TicketMetadata } from '../lib/types.ts';
import { buildTicketMap, exitWithError, formatTicketBullet } from './utils.ts';

export async function cmdShow(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk show <id>');
  }

  const ticket = await Ticket.find(positionals[0]);
  const content = await fs.readFile(ticket.filePath, 'utf-8');
  const metadata = await ticket.read();
  const ticketMap = await buildTicketMap();

  const blockers: TicketMetadata[] = [];
  const blocking: TicketMetadata[] = [];
  const children: TicketMetadata[] = [];

  for (const [id, t] of ticketMap) {
    if (id === ticket.id) continue;

    if (t.parent === ticket.id) {
      children.push(t);
    }

    if (t.deps?.includes(ticket.id) && t.status !== 'complete') {
      blocking.push(t);
    }
  }

  for (const depId of metadata.deps ?? []) {
    const dep = ticketMap.get(depId);
    if (dep && dep.status !== 'complete') {
      blockers.push(dep);
    }
  }

  console.log(content);

  const printSection = (title: string, items: TicketMetadata[]) => {
    if (items.length > 0) {
      console.log(`\n## ${title}`);
      for (const item of items) {
        console.log(formatTicketBullet(item));
      }
    }
  };

  printSection('Blockers', blockers);
  printSection('Blocking', blocking);
  printSection('Children', children);

  const links = metadata.links ?? [];
  if (links.length > 0) {
    console.log('\n## Linked');
    for (const linkId of links) {
      const t = ticketMap.get(linkId);
      if (t) {
        console.log(formatTicketBullet(t));
      }
    }
  }
}
