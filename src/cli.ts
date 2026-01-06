#!/usr/bin/env node

import {
  cmdCreate,
  cmdStart,
  cmdClose,
  cmdReopen,
  cmdStatus,
  cmdDep,
  cmdUndep,
  cmdLink,
  cmdUnlink,
  cmdLs,
  cmdReady,
  cmdBlocked,
  cmdClosed,
  cmdShow,
  cmdEdit,
  cmdAddNote,
  cmdQuery,
  cmdHelp,
  cmdMigrateBeads,
} from './commands/index.ts';

// ============================================================================
// Main
// ============================================================================

const commands: Record<string, (args: string[]) => Promise<void> | void> = {
  create: cmdCreate,
  start: cmdStart,
  close: cmdClose,
  reopen: cmdReopen,
  status: cmdStatus,
  dep: cmdDep,
  undep: cmdUndep,
  link: cmdLink,
  unlink: cmdUnlink,
  ls: cmdLs,
  ready: cmdReady,
  blocked: cmdBlocked,
  closed: cmdClosed,
  show: cmdShow,
  edit: cmdEdit,
  'add-note': cmdAddNote,
  query: cmdQuery,
  'migrate-beads': cmdMigrateBeads,
  help: cmdHelp,
};

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const cmd = args[0] ?? 'help';
  const cmdArgs = args.slice(1);

  const handler = commands[cmd];
  if (handler) {
    await handler(cmdArgs);
  } else {
    console.error(`Unknown command: ${cmd}`);
    cmdHelp();
    process.exit(1);
  }
}

main().catch((err) => {
  console.error(err.message);
  process.exit(1);
});
