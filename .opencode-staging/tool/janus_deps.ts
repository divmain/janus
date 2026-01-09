import { tool } from "@opencode-ai/plugin";

/**
 * Add a dependency to a ticket.
 */
export const add = tool({
  description: "Add a dependency (ticket A depends on ticket B).",
  args: {
    ticket_id: tool.schema.string().describe("The dependent ticket"),
    depends_on: tool.schema
      .string()
      .describe("Ticket that must be completed first"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus dep add ${args.ticket_id} ${args.depends_on} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Remove a dependency from a ticket.
 */
export const remove = tool({
  description: "Remove a dependency between tickets.",
  args: {
    ticket_id: tool.schema.string().describe("The dependent ticket"),
    depends_on: tool.schema.string().describe("Dependency to remove"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus dep remove ${args.ticket_id} ${args.depends_on} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Get the dependency tree for a ticket.
 */
export const tree = tool({
  description: "Get the dependency tree for a ticket.",
  args: {
    ticket_id: tool.schema.string().describe("Root ticket"),
    full: tool.schema
      .boolean()
      .optional()
      .describe("Show all occurrences (including duplicates)"),
  },
  async execute(args) {
    const cmdArgs: string[] = [args.ticket_id];

    if (args.full) {
      cmdArgs.push("--full");
    }
    cmdArgs.push("--json");

    try {
      const result = await Bun.$`janus dep tree ${cmdArgs}`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Create bidirectional links between tickets.
 */
export const link = tool({
  description: "Create bidirectional links between tickets.",
  args: {
    ticket_ids: tool.schema
      .array(tool.schema.string())
      .min(2)
      .describe("Two or more ticket IDs to link"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus link add ${args.ticket_ids} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});

/**
 * Remove bidirectional link between two tickets.
 */
export const unlink = tool({
  description: "Remove a bidirectional link between tickets.",
  args: {
    ticket_id_1: tool.schema.string().describe("First ticket"),
    ticket_id_2: tool.schema.string().describe("Second ticket"),
  },
  async execute(args) {
    try {
      const result =
        await Bun.$`janus link remove ${args.ticket_id_1} ${args.ticket_id_2} --json`.text();
      return JSON.parse(result);
    } catch (e: any) {
      throw new Error(e.stderr?.trim() || e.message || "Unknown error");
    }
  },
});
