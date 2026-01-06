import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import { isValidStatus } from '../lib/types.ts';
import { exitWithError, VALID_STATUSES } from './utils.ts';

export async function cmdStatusUpdate(id: string, status: string): Promise<void> {
  if (!isValidStatus(status)) {
    exitWithError(
      `Error: invalid status '${status}'. Must be one of: ${VALID_STATUSES.join(', ')}`
    );
  }

  const ticket = await Ticket.find(id);
  await ticket.updateField('status', status);
  console.log(`Updated ${ticket.id} -> ${status}`);
}

export async function cmdStart(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk start <id>');
  }

  return cmdStatusUpdate(positionals[0], 'new');
}

export async function cmdClose(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk close <id>');
  }

  return cmdStatusUpdate(positionals[0], 'complete');
}

export async function cmdReopen(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk reopen <id>');
  }

  return cmdStatusUpdate(positionals[0], 'new');
}

export async function cmdStatus(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 2) {
    exitWithError(`Usage: tk status <id> <status>\nValid statuses: ${VALID_STATUSES.join(', ')}`);
  }

  const [id, status] = positionals;
  return cmdStatusUpdate(id, status);
}
