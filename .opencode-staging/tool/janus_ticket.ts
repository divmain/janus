import { tool } from "@opencode-ai/plugin";

/**
 * Create a new ticket.
 */
export const create = tool({
  description:
    "Create a new ticket with title, description, and optional metadata.",
  args: {
    title: tool.schema.string().describe("Ticket title"),
    description: tool.schema
      .string()
      .optional()
      .describe("Description text"),
    type: tool.schema
      .enum(["bug", "feature", "task", "epic", "chore"])
      .optional()
      .describe("Ticket type"),
    priority: tool.schema
      .number()
      .min(0)
      .max(4)
      .optional()
      .describe("Priority (0=highest, 4=lowest)"),
    parent: tool.schema.string().optional().describe("Parent ticket ID"),
    design: tool.schema.string().optional().describe("Design notes"),
    acceptance: tool.schema
      .string()
      .optional()
      .describe("Acceptance criteria"),
    prefix: tool.schema.string().optional().describe("Custom ID prefix"),
  },
  async execute(args) {
    const cmdArgs = [args.title];

    if (args.description) {
      cmdArgs.push("--description", args.description);
    }
    if (args.type) {
      cmdArgs.push("--type", args.type);
    }
    if (args.priority !== undefined) {
      cmdArgs.push("--priority", String(args.priority));
    }
    if (args.parent) {
      cmdArgs.push("--parent", args.parent);
    }
    if (args.design) {
      cmdArgs.push("--design", args.design);
    }
    if (args.acceptance) {
      cmdArgs.push("--acceptance", args.acceptance);
    }
    if (args.prefix) {
      cmdArgs.push("--prefix", args.prefix);
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus create ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get ticket details with computed relationships.
 */
export const get = tool({
  description:
    "Get ticket details including status, dependencies, and relationships.",
  args: {
    id: tool.schema.string().describe("Ticket ID (can be partial)"),
  },
  async execute(args) {
    try {
      const result = await Bun.$`janus show ${args.id} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Update ticket fields.
 */
export const update = tool({
  description: "Update ticket fields such as status, priority, or type.",
  args: {
    id: tool.schema.string().describe("Ticket ID (can be partial)"),
    status: tool.schema
      .enum(["new", "next", "in_progress", "complete", "cancelled"])
      .optional()
      .describe("New status"),
    priority: tool.schema
      .number()
      .min(0)
      .max(4)
      .optional()
      .describe("Priority (0=highest, 4=lowest)"),
    type: tool.schema
      .enum(["bug", "feature", "task", "epic", "chore"])
      .optional()
      .describe("Ticket type"),
    parent: tool.schema
      .string()
      .optional()
      .describe("Parent ticket ID (empty string to clear)"),
  },
  async execute(args) {
    const results: Record<string, any> = {};
    const errors: string[] = [];

    // Update status if provided
    if (args.status) {
      try {
        const result =
          await Bun.$`janus status ${args.id} ${args.status} --json`.text();
        results.status = JSON.parse(result);
      } catch (e: any) {
        errors.push(
          `status: ${e.stderr?.trim() || e.message || "Unknown error"}`
        );
      }
    }

    // Update priority if provided
    if (args.priority !== undefined) {
      try {
        const result =
          await Bun.$`janus set ${args.id} priority ${args.priority} --json`.text();
        results.priority = JSON.parse(result);
      } catch (e: any) {
        errors.push(
          `priority: ${e.stderr?.trim() || e.message || "Unknown error"}`
        );
      }
    }

    // Update type if provided
    if (args.type) {
      try {
        const result =
          await Bun.$`janus set ${args.id} type ${args.type} --json`.text();
        results.type = JSON.parse(result);
      } catch (e: any) {
        errors.push(
          `type: ${e.stderr?.trim() || e.message || "Unknown error"}`
        );
      }
    }

    // Update parent if provided (including empty string to clear)
    if (args.parent !== undefined) {
      try {
        const parentValue = args.parent || "";
        const result =
          await Bun.$`janus set ${args.id} parent ${parentValue} --json`.text();
        results.parent = JSON.parse(result);
      } catch (e: any) {
        errors.push(
          `parent: ${e.stderr?.trim() || e.message || "Unknown error"}`
        );
      }
    }

    if (errors.length > 0) {
      throw new Error(`Update failed: ${errors.join("; ")}`);
    }

    // Return the most recent result (last update contains full ticket state)
    const lastResult = Object.values(results).pop();
    return lastResult || { id: args.id, message: "No fields updated" };
  },
});

/**
 * List tickets with filters.
 */
export const list = tool({
  description:
    "List tickets with optional filters for status, readiness, or blocked state.",
  args: {
    ready: tool.schema
      .boolean()
      .optional()
      .describe("Show ready tickets (no incomplete deps)"),
    blocked: tool.schema.boolean().optional().describe("Show blocked tickets"),
    closed: tool.schema
      .boolean()
      .optional()
      .describe("Show closed/cancelled tickets"),
    all: tool.schema
      .boolean()
      .optional()
      .describe("Include closed in results"),
    status: tool.schema
      .enum(["new", "next", "in_progress", "complete", "cancelled"])
      .optional()
      .describe("Filter by specific status"),
    limit: tool.schema
      .number()
      .optional()
      .describe("Maximum tickets to return"),
  },
  async execute(args) {
    const cmdArgs: string[] = [];

    if (args.ready) {
      cmdArgs.push("--ready");
    }
    if (args.blocked) {
      cmdArgs.push("--blocked");
    }
    if (args.closed) {
      cmdArgs.push("--closed");
    }
    if (args.all) {
      cmdArgs.push("--all");
    }
    if (args.status) {
      cmdArgs.push("--status", args.status);
    }
    if (args.limit !== undefined) {
      cmdArgs.push("--limit", String(args.limit));
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus ls ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Add a timestamped note to a ticket.
 */
export const add_note = tool({
  description: "Add a timestamped note to a ticket.",
  args: {
    id: tool.schema.string().describe("Ticket ID (can be partial)"),
    note: tool.schema.string().describe("Note text"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus add-note ${args.id} ${args.note} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Search tickets by field values.
 */
export const search = tool({
  description: "Search tickets by field values like status, type, or title.",
  args: {
    status: tool.schema
      .enum(["new", "next", "in_progress", "complete", "cancelled"])
      .optional()
      .describe("Filter by status"),
    type: tool.schema
      .enum(["bug", "feature", "task", "epic", "chore"])
      .optional()
      .describe("Filter by type"),
    priority: tool.schema
      .number()
      .min(0)
      .max(4)
      .optional()
      .describe("Filter by priority"),
    title_contains: tool.schema
      .string()
      .optional()
      .describe("Filter by title substring (case-insensitive)"),
  },
  async execute(args) {
    try {
      // Get all tickets as JSON
      const result = await Bun.$`janus query`.text();
      const lines = result.trim().split("\n").filter(Boolean);
      let tickets = lines.map((line) => JSON.parse(line));

      // Apply filters in TypeScript
      if (args.status) {
        tickets = tickets.filter((t) => t.status === args.status);
      }
      if (args.type) {
        tickets = tickets.filter((t) => t.type === args.type);
      }
      if (args.priority !== undefined) {
        tickets = tickets.filter((t) => t.priority === args.priority);
      }
      if (args.title_contains) {
        const searchLower = args.title_contains.toLowerCase();
        tickets = tickets.filter((t) =>
          t.title.toLowerCase().includes(searchLower)
        );
      }

      return tickets;
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});
