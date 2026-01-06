export const TICKETS_DIR = '.janus';

export type TicketStatus = 'new' | 'cancelled' | 'complete';
export type TicketType = 'bug' | 'feature' | 'task' | 'epic' | 'chore';
export type TicketPriority = '0' | '1' | '2' | '3' | '4';

export const VALID_STATUSES: readonly TicketStatus[] = ['new', 'cancelled', 'complete'];
export const VALID_TYPES: readonly TicketType[] = ['bug', 'feature', 'task', 'epic', 'chore'];
export const VALID_PRIORITIES: readonly TicketPriority[] = ['0', '1', '2', '3', '4'];

export interface TicketMetadata {
  id?: string;
  title?: string;
  status?: TicketStatus;
  deps?: string[];
  links?: string[];
  created?: string;
  type?: TicketType;
  priority?: TicketPriority;
  assignee?: string;
  'external-ref'?: string;
  parent?: string;
  filePath?: string;
  [key: string]: unknown;
}

export function isValidStatus(status: string): status is TicketStatus {
  return VALID_STATUSES.includes(status as TicketStatus);
}

export function isValidType(type: string): type is TicketType {
  return VALID_TYPES.includes(type as TicketType);
}

export function isValidPriority(priority: string): priority is TicketPriority {
  return VALID_PRIORITIES.includes(priority as TicketPriority);
}
