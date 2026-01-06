import fs from 'node:fs/promises';
import path from 'node:path';

import { parseTicketContent } from './parser.ts';
import { TICKETS_DIR, type TicketMetadata } from './types.ts';

async function findTickets(): Promise<string[]> {
  try {
    const files = await fs.readdir(TICKETS_DIR);
    return files.filter((f) => f.endsWith('.md'));
  } catch {
    return [];
  }
}

async function findTicketById(partialId: string): Promise<string> {
  const files = await findTickets();

  // Check for exact match first
  const exact = files.find((f) => f === `${partialId}.md`);
  if (exact) {
    return path.join(TICKETS_DIR, exact);
  }

  // Then check for partial matches
  const matches = files.filter((f) => f.includes(partialId));

  if (matches.length === 0) {
    throw new Error(`ticket '${partialId}' not found`);
  }

  if (matches.length > 1) {
    throw new Error(`ambiguous ID '${partialId}' matches multiple tickets`);
  }

  return path.join(TICKETS_DIR, matches[0]);
}

export class Ticket {
  readonly filePath: string;
  readonly id: string;

  static async find(partialId: string): Promise<Ticket> {
    const filePath = await findTicketById(partialId);
    return new Ticket(filePath);
  }

  constructor(filePath: string) {
    this.filePath = filePath;
    this.id = path.basename(filePath, '.md');
  }

  async read(): Promise<TicketMetadata> {
    const content = await fs.readFile(this.filePath, 'utf-8');
    return parseTicketContent(content);
  }

  async write(content: string): Promise<void> {
    await fs.mkdir(path.dirname(this.filePath), { recursive: true });
    await fs.writeFile(this.filePath, content, 'utf-8');
  }

  async updateField(field: string, value: string): Promise<void> {
    const content = await fs.readFile(this.filePath, 'utf-8');
    const fieldPattern = new RegExp(`^${field}:\\s*.*$`, 'm');

    const newContent = fieldPattern.test(content)
      ? content.replace(fieldPattern, `${field}: ${value}`)
      : content.replace(/^---$\n/m, `---\n${field}: ${value}\n`);

    await fs.writeFile(this.filePath, newContent, 'utf-8');
  }

  async addToArrayField(field: string, value: string): Promise<boolean> {
    const current = await this.read();
    const currentArray = (current[field] as string[]) ?? [];

    if (currentArray.includes(value)) {
      return false;
    }

    const newArray = [...currentArray, value];
    await this.updateField(field, JSON.stringify(newArray));
    return true;
  }

  async removeFromArrayField(field: string, value: string): Promise<boolean> {
    const current = await this.read();
    const currentArray = (current[field] as string[]) ?? [];

    if (!currentArray.includes(value)) {
      return false;
    }

    const newArray = currentArray.filter((v) => v !== value);
    await this.updateField(field, newArray.length === 0 ? '[]' : JSON.stringify(newArray));
    return true;
  }
}

export async function getAllTickets(): Promise<TicketMetadata[]> {
  const files = await findTickets();
  const tickets: TicketMetadata[] = [];

  for (const file of files) {
    try {
      const filePath = path.join(TICKETS_DIR, file);
      const content = await fs.readFile(filePath, 'utf-8');
      const metadata = parseTicketContent(content);
      metadata.id = file.slice(0, -3);
      metadata.filePath = filePath;
      tickets.push(metadata);
    } catch (err) {
      console.warn(`Warning: failed to parse ${file}: ${(err as Error).message}`);
    }
  }

  return tickets;
}
