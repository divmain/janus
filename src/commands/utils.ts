import { createInterface } from 'node:readline';

import { getAllTickets } from '../lib/ticket.ts';
import { VALID_STATUSES, type TicketMetadata } from '../lib/types.ts';

export { VALID_STATUSES };

export interface TicketWithBlockers extends TicketMetadata {
  openBlockers: string[];
}

// Helper to exit with error
export function exitWithError(message: string, code = 1): never {
  console.error(message);
  process.exit(code);
}

// Read all input from stdin
export function readStdin(): Promise<string> {
  return new Promise((resolve) => {
    const rl = createInterface({
      input: process.stdin,
      crlfDelay: Infinity,
    });
    const lines: string[] = [];
    rl.on('line', (line) => lines.push(line));
    rl.on('close', () => resolve(lines.join('\n').trim()));
  });
}

// Build a map of all tickets by ID
export async function buildTicketMap(): Promise<Map<string, TicketMetadata>> {
  const tickets = await getAllTickets();
  return new Map(tickets.filter((t) => t.id).map((t) => [t.id!, t]));
}

// Sort tickets by priority (ascending) then by ID
export function sortByPriority<T extends TicketMetadata>(tickets: T[]): T[] {
  return tickets.sort((a, b) => {
    const pA = parseInt(a.priority ?? '2', 10) || 2;
    const pB = parseInt(b.priority ?? '2', 10) || 2;
    if (pA !== pB) return pA - pB;
    return (a.id ?? '').localeCompare(b.id ?? '');
  });
}

// Format a ticket for display
export interface FormatTicketOptions {
  showPriority?: boolean;
  suffix?: string;
}

export function formatTicketLine(ticket: TicketMetadata, options: FormatTicketOptions = {}): string {
  const id = (ticket.id ?? '???').padEnd(8);
  const priority = options.showPriority ? `[P${ticket.priority}]` : '';
  const status = `[${ticket.status}]`;
  const title = ticket.title ?? '';
  const suffix = options.suffix ?? '';

  return `${id} ${priority}${status} - ${title}${suffix}`;
}

// Format dependencies for display
export function formatDeps(deps: string[]): string {
  const depsStr = deps.join(', ');
  return depsStr ? ` <- [${depsStr}]` : ' <- []';
}

// Format a ticket as a bullet point (for show command sections)
export function formatTicketBullet(ticket: TicketMetadata): string {
  return `- ${ticket.id} [${ticket.status}] ${ticket.title}`;
}
