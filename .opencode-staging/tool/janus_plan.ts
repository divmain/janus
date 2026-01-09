import { tool } from "@opencode-ai/plugin";

/**
 * Create a new plan.
 */
export const create = tool({
  description: "Create a new simple or phased plan.",
  args: {
    title: tool.schema.string().describe("Plan title"),
    phases: tool.schema
      .array(tool.schema.string())
      .optional()
      .describe("Phase names (creates phased plan if provided)"),
  },
  async execute(args) {
    const cmdArgs = [args.title];

    if (args.phases && args.phases.length > 0) {
      for (const phase of args.phases) {
        cmdArgs.push("--phase", phase);
      }
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan create ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get plan details with computed status and progress.
 */
export const get = tool({
  description: "Get plan details with computed status and progress.",
  args: {
    id: tool.schema.string().describe("Plan ID (can be partial)"),
    tickets_only: tool.schema
      .boolean()
      .optional()
      .describe("Return only ticket list"),
    phases_only: tool.schema
      .boolean()
      .optional()
      .describe("Return only phase summary"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.id];

    if (args.tickets_only) {
      cmdArgs.push("--tickets-only");
    }
    if (args.phases_only) {
      cmdArgs.push("--phases-only");
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan show ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * List all plans.
 */
export const list = tool({
  description: "List all plans with optional status filter.",
  args: {
    status: tool.schema
      .enum(["new", "in_progress", "complete", "cancelled"])
      .optional()
      .describe("Filter by computed status"),
  },
  async execute(args) {
    const cmdArgs: string[] = [];

    if (args.status) {
      cmdArgs.push("--status", args.status);
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan ls ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Rename a plan.
 */
export const update = tool({
  description: "Rename a plan.",
  args: {
    id: tool.schema.string().describe("Plan ID (can be partial)"),
    new_title: tool.schema.string().describe("New plan title"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus plan rename ${args.id} ${args.new_title} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Delete a plan.
 */
export const del = tool({
  description: "Delete a plan (does not delete contained tickets).",
  args: {
    id: tool.schema.string().describe("Plan ID (can be partial)"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus plan delete ${args.id} --force --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Add a ticket to a plan.
 */
export const add_ticket = tool({
  description: "Add a ticket to a plan or phase.",
  args: {
    plan_id: tool.schema.string().describe("Plan ID"),
    ticket_id: tool.schema.string().describe("Ticket ID to add"),
    phase: tool.schema
      .string()
      .optional()
      .describe("Target phase (required for phased plans)"),
    after: tool.schema
      .string()
      .optional()
      .describe("Insert after this ticket"),
    position: tool.schema
      .number()
      .optional()
      .describe("Insert at position (1-indexed)"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.plan_id, args.ticket_id];

    if (args.phase) {
      cmdArgs.push("--phase", args.phase);
    }
    if (args.after) {
      cmdArgs.push("--after", args.after);
    }
    if (args.position !== undefined) {
      cmdArgs.push("--position", String(args.position));
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan add-ticket ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Remove a ticket from a plan.
 */
export const remove_ticket = tool({
  description: "Remove a ticket from a plan.",
  args: {
    plan_id: tool.schema.string().describe("Plan ID"),
    ticket_id: tool.schema.string().describe("Ticket ID to remove"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus plan remove-ticket ${args.plan_id} ${args.ticket_id} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Move a ticket between phases.
 */
export const move_ticket = tool({
  description: "Move a ticket between phases in a phased plan.",
  args: {
    plan_id: tool.schema.string().describe("Plan ID"),
    ticket_id: tool.schema.string().describe("Ticket ID to move"),
    to_phase: tool.schema.string().describe("Target phase"),
    after: tool.schema
      .string()
      .optional()
      .describe("Insert after this ticket"),
    position: tool.schema
      .number()
      .optional()
      .describe("Insert at position (1-indexed)"),
  },
  async execute(args) {
    const cmdArgs: string[] = [
      args.plan_id,
      args.ticket_id,
      "--to-phase",
      args.to_phase,
    ];

    if (args.after) {
      cmdArgs.push("--after", args.after);
    }
    if (args.position !== undefined) {
      cmdArgs.push("--position", String(args.position));
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan move-ticket ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Add a phase to a plan.
 */
export const add_phase = tool({
  description: "Add a new phase to a plan.",
  args: {
    plan_id: tool.schema.string().describe("Plan ID"),
    phase_name: tool.schema.string().describe("Name for new phase"),
    after: tool.schema.string().optional().describe("Insert after this phase"),
    position: tool.schema
      .number()
      .optional()
      .describe("Insert at position (1-indexed)"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.plan_id, args.phase_name];

    if (args.after) {
      cmdArgs.push("--after", args.after);
    }
    if (args.position !== undefined) {
      cmdArgs.push("--position", String(args.position));
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan add-phase ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Remove a phase from a plan.
 */
export const remove_phase = tool({
  description: "Remove a phase from a plan.",
  args: {
    plan_id: tool.schema.string().describe("Plan ID"),
    phase: tool.schema.string().describe("Phase name or number"),
    force: tool.schema
      .boolean()
      .optional()
      .describe("Force removal even if phase has tickets"),
    migrate_to: tool.schema
      .string()
      .optional()
      .describe("Move tickets to this phase before removing"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.plan_id, args.phase];

    if (args.force) {
      cmdArgs.push("--force");
    }
    if (args.migrate_to) {
      cmdArgs.push("--migrate", args.migrate_to);
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan remove-phase ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get next actionable items from a plan.
 */
export const next = tool({
  description: "Get the next actionable items from a plan.",
  args: {
    id: tool.schema.string().describe("Plan ID"),
    count: tool.schema
      .number()
      .optional()
      .describe("Number of items to return (default: 1)"),
    all_phases: tool.schema
      .boolean()
      .optional()
      .describe("Show next from all incomplete phases"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.id];

    if (args.count !== undefined) {
      cmdArgs.push("--count", String(args.count));
    }
    if (args.all_phases) {
      cmdArgs.push("--all");
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus plan next ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get plan status summary with phase breakdown.
 */
export const status = tool({
  description: "Get plan status summary with phase breakdown.",
  args: {
    id: tool.schema.string().describe("Plan ID"),
  },
  async execute(args) {
    try {
      const result = await Bun.$`janus plan status ${args.id} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Import a plan from markdown content, creating tickets.
 */
export const import_plan = tool({
  description: "Import a plan from markdown, creating tickets automatically.",
  args: {
    content: tool.schema.string().describe("Markdown content to import"),
    title: tool.schema.string().optional().describe("Override extracted title"),
    type: tool.schema
      .enum(["bug", "feature", "task", "epic", "chore"])
      .optional()
      .describe("Ticket type for created tasks"),
    prefix: tool.schema.string().optional().describe("Custom ticket ID prefix"),
    dry_run: tool.schema
      .boolean()
      .optional()
      .describe("Validate only, don't create"),
  },
  async execute(args) {
    const cmdArgs: string[] = ["-"];

    if (args.title) {
      cmdArgs.push("--title", args.title);
    }
    if (args.type) {
      cmdArgs.push("--type", args.type);
    }
    if (args.prefix) {
      cmdArgs.push("--prefix", args.prefix);
    }
    if (args.dry_run) {
      cmdArgs.push("--dry-run");
    }
    cmdArgs.push("--json");

    try {
      const result =
        await Bun.$`echo ${args.content} | janus plan import ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get the importable plan format specification.
 */
export const import_spec = tool({
  description:
    "Get the format specification for importable plan documents.",
  args: {},
  async execute() {
    try {
      const result = await Bun.$`janus plan import-spec`.text();
      return { spec: result };
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});
