import { Err, Ok, type Result } from './result.ts';
import {
  isValidPriority,
  isValidStatus,
  isValidType,
  type TicketMetadata,
  type TicketPriority,
  type TicketStatus,
  type TicketType,
} from './types.ts';

export function parseTicketContent(content: string): TicketMetadata {
  const frontmatterMatch = content.match(/^---\n(.*?)\n---\n(.*)$/s);

  if (!frontmatterMatch) {
    throw new Error('Invalid ticket format: missing YAML frontmatter');
  }

  const [, yaml, body] = frontmatterMatch;
  const metadata: TicketMetadata = {};

  for (const line of yaml.split('\n')) {
    const match = line.match(/^(\w[-\w]*):\s*(.*)$/);
    if (match) {
      const [, key, value] = match;
      // Parse JSON arrays, keep everything else as strings
      const parsedValue = value.startsWith('[') && value.endsWith(']') ? JSON.parse(value) : value;

      // Type-check known fields
      switch (key) {
        case 'status':
          metadata.status = isValidStatus(parsedValue) ? parsedValue : (parsedValue as TicketStatus);
          break;
        case 'type':
          metadata.type = isValidType(parsedValue) ? parsedValue : (parsedValue as TicketType);
          break;
        case 'priority':
          metadata.priority = isValidPriority(parsedValue) ? parsedValue : (parsedValue as TicketPriority);
          break;
        default:
          metadata[key] = parsedValue;
      }
    }
  }

  const titleMatch = body.match(/^#\s+(.*)$/m);
  if (titleMatch) {
    metadata.title = titleMatch[1];
  }

  return metadata;
}

/**
 * Safe version of parseTicketContent that returns a Result instead of throwing.
 */
export function tryParseTicketContent(content: string): Result<TicketMetadata, Error> {
  try {
    return Ok(parseTicketContent(content));
  } catch (e) {
    return Err(e instanceof Error ? e : new Error(String(e)));
  }
}
