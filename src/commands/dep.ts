import { parseArgs } from 'node:util';

import { Ticket } from '../lib/ticket.ts';
import { buildTicketMap, exitWithError } from './utils.ts';

export async function cmdDepTree(args: string[]): Promise<void> {
  const { values, positionals } = parseArgs({
    args,
    options: {
      full: { type: 'boolean', default: false },
    },
    allowPositionals: true,
  });

  const rootId = positionals[0];
  if (!rootId) {
    exitWithError('Usage: tk dep tree [--full] <id>');
  }

  const ticketMap = await buildTicketMap();
  const matchingIds = [...ticketMap.keys()].filter((id) => id.includes(rootId));

  if (matchingIds.length === 0) {
    exitWithError(`Error: ticket ${rootId} not found`);
  }
  if (matchingIds.length > 1) {
    exitWithError(`Error: ambiguous ID ${rootId}`);
  }

  const root = matchingIds[0];
  const fullMode = values.full;

  // Calculate the maximum depth at which each node appears
  const maxDepth = new Map<string, number>();
  const subtreeDepth = new Map<string, number>();
  const visited = new Set<string>();

  function findMaxDepth(id: string, currentDepth = 0, path = new Set<string>()): void {
    if (path.has(id)) return;

    maxDepth.set(id, Math.max(maxDepth.get(id) ?? 0, currentDepth));

    for (const dep of ticketMap.get(id)?.deps ?? []) {
      if (dep && !path.has(dep)) {
        findMaxDepth(dep, currentDepth + 1, new Set([...path, id]));
      }
    }
  }

  function computeSubtreeDepth(id: string): number {
    let max = maxDepth.get(id) ?? 0;
    for (const dep of ticketMap.get(id)?.deps ?? []) {
      if (dep) {
        max = Math.max(max, computeSubtreeDepth(dep));
      }
    }
    subtreeDepth.set(id, max);
    return max;
  }

  findMaxDepth(root);
  computeSubtreeDepth(root);

  function getPrintableChildren(id: string, depth: number): string[] {
    const deps = ticketMap.get(id)?.deps ?? [];
    const children = deps.filter((dep) => {
      if (!dep || !maxDepth.has(dep)) return false;
      return fullMode || depth + 1 === maxDepth.get(dep);
    });

    return children.sort((a, b) => {
      const depthDiff = (subtreeDepth.get(b) ?? 0) - (subtreeDepth.get(a) ?? 0);
      return depthDiff !== 0 ? depthDiff : a.localeCompare(b);
    });
  }

  function printTree(id: string, depth = 0, prefix = ''): void {
    const children = getPrintableChildren(id, depth);

    for (let i = 0; i < children.length; i++) {
      const child = children[i];
      const isLast = i === children.length - 1;
      const connector = isLast ? '└── ' : '├── ';
      const childPrefix = isLast ? '    ' : '│   ';
      const ticket = ticketMap.get(child);

      console.log(`${prefix}${connector}${child} [${ticket?.status ?? '?'}] ${ticket?.title ?? ''}`);

      if (!fullMode) visited.add(child);
      printTree(child, depth + 1, prefix + childPrefix);
    }
  }

  const rootTicket = ticketMap.get(root);
  console.log(`${root} [${rootTicket?.status ?? '?'}] ${rootTicket?.title ?? ''}`);
  visited.add(root);
  printTree(root);
}

export async function cmdDep(args: string[]): Promise<void> {
  if (args[0] === 'tree') {
    return cmdDepTree(args.slice(1));
  }

  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 2) {
    exitWithError('Usage: tk dep <id> <dep-id>\n       tk dep tree <id>  - show dependency tree');
  }

  const [id, depId] = positionals;
  const ticket = await Ticket.find(id);
  await Ticket.find(depId); // Validate dep exists

  const added = await ticket.addToArrayField('deps', depId);
  console.log(added ? `Added dependency: ${ticket.id} -> ${depId}` : 'Dependency already exists');
}

export async function cmdUndep(args: string[]): Promise<void> {
  const { positionals } = parseArgs({ args, allowPositionals: true });

  if (positionals.length < 2) {
    exitWithError('Usage: tk undep <id> <dependency-id>');
  }

  const [id, depId] = positionals;
  const ticket = await Ticket.find(id);
  const removed = await ticket.removeFromArrayField('deps', depId);

  if (!removed) {
    exitWithError('Dependency not found');
  }

  console.log(`Removed dependency: ${ticket.id} -/-> ${depId}`);
}
