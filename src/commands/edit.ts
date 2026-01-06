import { spawn } from 'node:child_process';
import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import { exitWithError } from './utils.ts';

export async function cmdEdit(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 1) {
    exitWithError('Usage: tk edit <id>');
  }

  const ticket = await Ticket.find(positionals[0]);

  if (process.stdin.isTTY) {
    const editor = process.env.EDITOR || 'vi';
    await new Promise<void>((resolve, reject) => {
      const proc = spawn(editor, [ticket.filePath], { stdio: 'inherit' });
      proc.on('close', (code) => {
        code === 0 ? resolve() : reject(new Error(`Editor exited with code ${code}`));
      });
    });
  } else {
    console.log(`Edit ticket file: ${ticket.filePath}`);
  }
}
