# MCP Server

Janus includes an MCP (Model Context Protocol) server that allows AI agents to interact with your tickets and plans. The server uses STDIO transport for local integration with AI assistants.

## Starting the MCP Server

```bash
# Start the MCP server (runs until terminated)
janus mcp

# Show MCP protocol version
janus mcp --version
```

## Available Tools

The MCP server exposes 13 tools for ticket and plan management:

| Tool | Description |
|------|-------------|
| `create_ticket` | Create a new ticket with title, type, priority, and size |
| `spawn_subtask` | Create a child ticket with spawning metadata for decomposition tracking |
| `update_status` | Change ticket status (new/next/in_progress/complete/cancelled) |
| `add_note` | Add a timestamped note to a ticket |
| `list_tickets` | Query tickets with filters (status, type, ready, blocked, etc.) |
| `show_ticket` | Get full ticket content including metadata, body, dependencies, and relationships |
| `add_dependency` | Add a blocking dependency between tickets |
| `remove_dependency` | Remove a dependency from a ticket |
| `add_ticket_to_plan` | Add a ticket to a plan (with optional phase for phased plans) |
| `get_plan_status` | Get plan progress including percentage and phase breakdown |
| `get_children` | Get all tickets spawned from a parent ticket |
| `get_next_available_ticket` | Query the backlog for the next ticket(s) to work on |
| `semantic_search` | Find tickets semantically similar to a query (requires semantic-search feature) |

## Available Resources

The MCP server exposes 9 resources for read-only access to Janus data:

### Static Resources

| URI | Description | MIME Type |
|-----|-------------|-----------|
| `janus://tickets/ready` | Tickets ready to work on (new/next with all deps complete) | application/json |
| `janus://tickets/blocked` | Tickets blocked by incomplete dependencies | application/json |
| `janus://tickets/in-progress` | Tickets currently being worked on | application/json |
| `janus://graph/deps` | Dependency graph in DOT format | text/vnd.graphviz |
| `janus://graph/spawning` | Spawning (parent/child) graph in DOT format | text/vnd.graphviz |

### Resource Templates (with parameters)

| URI Pattern | Description | MIME Type |
|-------------|-------------|-----------|
| `janus://ticket/{id}` | Full markdown content of a specific ticket | text/markdown |
| `janus://plan/{id}` | Plan details with computed status and phases | application/json |
| `janus://plan/{id}/next` | Next actionable items in a plan | application/json |
| `janus://tickets/spawned-from/{id}` | Children of a specific parent ticket | application/json |

## Example MCP Usage

When connected to an AI assistant via MCP:

```
# The AI can create tickets
-> create_ticket({"title": "Fix login bug", "type": "bug", "priority": 1})
<- Created ticket **j-a1b2**: "Fix login bug"

# Query ready tickets
-> list_tickets({"ready": true})
<- | ID | Title | Status | Type | Priority |
   | j-a1b2 | Fix login bug | new | bug | P1 |

# Update status
-> update_status({"id": "j-a1b2", "status": "in_progress"})
<- Updated **j-a1b2** status: new -> in_progress

# Read ticket content via resource
-> read_resource("janus://ticket/j-a1b2")
<- Full markdown content with frontmatter...
```

## Integration with AI Assistants

To use the Janus MCP server with an AI assistant:

1. Configure your AI assistant to use the Janus MCP server
2. Point it to the `janus mcp` command
3. The assistant can then call tools and read resources to interact with your tickets

### Claude Desktop

Example configuration for Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "janus": {
      "command": "/path/to/janus",
      "args": ["mcp"]
    }
  }
}
```

### Claude Code

Claude Code uses the `claude mcp add` command to configure MCP servers with different scopes:

```bash
# Add Janus as a local stdio MCP server (default: local scope)
claude mcp add --transport stdio janus -- /path/to/janus mcp

# Add to project scope (shared via .mcp.json)
claude mcp add --transport stdio janus --scope project -- /path/to/janus mcp

# Add to user scope (available across all projects)
claude mcp add --transport stdio janus --scope user -- /path/to/janus mcp
```

You can also add it via JSON configuration:

```bash
claude mcp add-json janus '{"type":"stdio","command":"/path/to/janus","args":["mcp"]}'
```

Manage your MCP servers:

```bash
# List configured servers
claude mcp list

# Get details for a specific server
claude mcp get janus

# Remove the server
claude mcp remove janus

# Check server status within Claude Code
/mcp
```

### OpenCode

OpenCode uses a JSON configuration file (`opencode.json` or `opencode.jsonc`) to define MCP servers:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "janus": {
      "type": "local",
      "command": ["/path/to/janus", "mcp"],
      "enabled": true
    }
  }
}
```

You can also disable/enable servers without removing them:

```json
{
  "mcp": {
    "janus": {
      "type": "local",
      "command": ["/path/to/janus", "mcp"],
      "enabled": false
    }
  }
}
```

MCP tools are automatically available alongside built-in tools. You can manage them globally or per-agent:

```json
{
  "mcp": {
    "janus": {
      "type": "local",
      "command": ["/path/to/janus", "mcp"]
    }
  },
  "tools": {
    "janus*": false
  },
  "agent": {
    "my-agent": {
      "tools": {
        "janus*": true
      }
    }
  }
}
```
