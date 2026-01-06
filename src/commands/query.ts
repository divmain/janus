import { spawn } from 'node:child_process';
import { parseArgs } from 'node:util';

import { getAllTickets } from '../lib/ticket.ts';

export async function cmdQuery(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  const tickets = await getAllTickets();
  const filter = positionals[0] || '';
  const output = tickets.map((t) => JSON.stringify(t)).join('\n');

  if (filter) {
    const proc = spawn('jq', ['-c', `select(${filter})`], {
      stdio: ['pipe', 'inherit', 'inherit'],
    });
    proc.stdin.write(output);
    proc.stdin.end();
    await new Promise<void>((resolve) => proc.on('close', resolve));
  } else {
    console.log(output);
  }
}
