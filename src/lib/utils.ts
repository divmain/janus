import { execFile } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs/promises';
import path from 'node:path';
import { promisify } from 'node:util';

import { TICKETS_DIR } from './types.ts';

const execFileAsync = promisify(execFile);

export async function ensureDir(): Promise<void> {
  await fs.mkdir(TICKETS_DIR, { recursive: true });
}

export async function getGitUserName(): Promise<string> {
  try {
    const { stdout } = await execFileAsync('git', ['config', 'user.name']);
    return stdout.trim();
  } catch {
    return '';
  }
}

export function generateId(): string {
  const dirName = path.basename(process.cwd());

  // Generate prefix from directory name (first letter of each word)
  let prefix = dirName
    .replace(/[-_]/g, ' ')
    .split(' ')
    .map((word) => word[0])
    .filter(Boolean)
    .join('');

  if (!prefix) {
    prefix = dirName.slice(0, 3);
  }

  const hash = createHash('sha256')
    .update(randomBytes(16))
    .digest('hex')
    .slice(0, 4);

  return `${prefix}-${hash}`;
}

export function isoDate(): string {
  return new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
}
