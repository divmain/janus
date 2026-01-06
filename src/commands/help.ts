import path from 'node:path';

export function cmdHelp(): void {
  const cmd = path.basename(process.argv[1]);
  console.log(`${cmd} - minimal ticket system with dependency tracking

Usage: ${cmd} <command> [args]

Commands:
  create [title] [options] Create ticket, prints ID
    -d, --description      Description text
    --design               Design notes
    --acceptance           Acceptance criteria
    -t, --type             Type (bug|feature|task|epic|chore) [default: task]
    -p, --priority         Priority 0-4, 0=highest [default: 2]
    -a, --assignee         Assignee
    --external-ref         External reference (e.g., gh-123, JIRA-456)
    --parent               Parent ticket ID
  start <id>               Set status to new (alias for reopen)
  close <id>               Set status to complete
  reopen <id>              Set status to new
  status <id> <status>     Update status (new|cancelled|complete)
  dep <id> <dep-id>        Add dependency (id depends on dep-id)
  dep tree [--full] <id>   Show dependency tree (--full disables dedup)
  undep <id> <dep-id>      Remove dependency
  link <id> <id> [id...]   Link tickets together (symmetric)
  unlink <id> <target-id>  Remove link between tickets
  ls [--status=X]          List tickets
  ready                    List new tickets with deps resolved
  blocked                  List new tickets with unresolved deps
  closed [--limit=N]       List recently completed tickets (default 20, by mtime)
  show <id>                Display ticket
  edit <id>                Open ticket in $EDITOR
  add-note <id> [text]     Append timestamped note (or pipe via stdin)
  query [jq-filter]        Output tickets as JSON, optionally filtered
  migrate-beads            Import tickets from .beads/issues.jsonl

Tickets stored as markdown files in .janus/
Supports partial ID matching (e.g., '${cmd} show 5c4' matches 'nw-5c46')`);
}
