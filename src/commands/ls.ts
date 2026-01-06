import fs from 'node:fs/promises';
import path from 'node:path';
import { parseArgs } from 'node:util';

import { parseTicketContent } from '../lib/parser.ts';
import { getAllTickets } from '../lib/ticket.ts';
import { TICKETS_DIR, type TicketMetadata } from '../lib/types.ts';
import { buildTicketMap, formatDeps, formatTicketLine, sortByPriority, type TicketWithBlockers } from './utils.ts';

export async function cmdLs(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: {
      status: { type: 'string' },
    },
  });

  const tickets = await getAllTickets();

  for (const t of tickets) {
    if (values.status && t.status !== values.status) continue;
    console.log(formatTicketLine(t, { suffix: formatDeps(t.deps ?? []) }));
  }
}

export async function cmdReady(): Promise<void> {
  const ticketMap = await buildTicketMap();
  const tickets = [...ticketMap.values()];

  const ready = tickets.filter((t) => {
    if (t.status !== 'new') return false;

    return (t.deps ?? []).every((depId) => {
      const dep = ticketMap.get(depId);
      return dep?.status === 'complete';
    });
  });

  sortByPriority(ready);

  for (const t of ready) {
    console.log(formatTicketLine(t, { showPriority: true }));
  }
}

export async function cmdBlocked(): Promise<void> {
  const ticketMap = await buildTicketMap();
  const tickets = [...ticketMap.values()];

  const blocked: TicketWithBlockers[] = [];

  for (const t of tickets) {
    if (t.status !== 'new') continue;
    if (!t.deps?.length) continue;

    const openBlockers = t.deps.filter((depId) => {
      const dep = ticketMap.get(depId);
      return !dep || dep.status !== 'complete';
    });

    if (openBlockers.length > 0) {
      blocked.push({ ...t, openBlockers });
    }
  }

  sortByPriority(blocked);

  for (const t of blocked) {
    console.log(formatTicketLine(t, { showPriority: true, suffix: formatDeps(t.openBlockers) }));
  }
}

export async function cmdClosed(args: string[]): Promise<void> {
  const { values } = parseArgs({
    args,
    options: {
      limit: { type: 'string', default: '20' },
    },
  });

  const limit = parseInt(values.limit ?? '20', 10) || 20;

  const files = await fs.readdir(TICKETS_DIR).catch(() => []);
  const mdFiles = files.filter((f) => f.endsWith('.md'));

  const fileStats = await Promise.all(
    mdFiles.map(async (file) => {
      const filePath = path.join(TICKETS_DIR, file);
      const stats = await fs.stat(filePath);
      return { file, filePath, mtime: stats.mtime.getTime() };
    })
  );

  fileStats.sort((a, b) => b.mtime - a.mtime);

  const closedTickets: TicketMetadata[] = [];

  for (const { filePath } of fileStats.slice(0, limit * 2)) {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      const metadata = parseTicketContent(content);
      if (metadata.status === 'complete') {
        metadata.filePath = filePath;
        closedTickets.push(metadata);
        if (closedTickets.length >= limit) break;
      }
    } catch {
      continue;
    }
  }

  for (const t of closedTickets) {
    // For closed tickets, use the id from filePath if not set
    if (!t.id) {
      t.id = path.basename(t.filePath ?? '???', '.md');
    }
    console.log(formatTicketLine(t));
  }
}
