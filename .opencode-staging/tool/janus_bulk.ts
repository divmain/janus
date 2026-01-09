import { tool } from "@opencode-ai/plugin";

/**
 * Update status of multiple tickets at once.
 */
export const update_status = tool({
  description: "Update status of multiple tickets at once.",
  args: {
    ticket_ids: tool.schema
      .array(tool.schema.string())
      .min(1)
      .describe("Array of ticket IDs"),
    status: tool.schema
      .enum(["new", "next", "in_progress", "complete", "cancelled"])
      .describe("Status to set for all tickets"),
  },
  async execute(args) {
    const succeeded: string[] = [];
    const failed: { id: string; error: string }[] = [];

    for (const id of args.ticket_ids) {
      try {
        await Bun.$`janus status ${id} ${args.status} --json`.text();
        succeeded.push(id);
      } catch (e: any) {
        failed.push({
          id,
          error: e.stderr?.trim() || e.message || "Unknown error",
        });
      }
    }

    return { succeeded, failed };
  },
});

/**
 * Add multiple dependencies to a ticket.
 */
export const add_deps = tool({
  description: "Add multiple dependencies to a ticket.",
  args: {
    ticket_id: tool.schema.string().describe("The dependent ticket"),
    depends_on: tool.schema
      .array(tool.schema.string())
      .min(1)
      .describe("Array of tickets it depends on"),
  },
  async execute(args) {
    const succeeded: string[] = [];
    const failed: { id: string; error: string }[] = [];

    for (const dep of args.depends_on) {
      try {
        await Bun.$`janus dep add ${args.ticket_id} ${dep} --json`.text();
        succeeded.push(dep);
      } catch (e: any) {
        failed.push({
          id: dep,
          error: e.stderr?.trim() || e.message || "Unknown error",
        });
      }
    }

    return { succeeded, failed };
  },
});

/**
 * Create multiple tickets at once.
 */
export const create_tickets = tool({
  description: "Create multiple tickets at once.",
  args: {
    tickets: tool.schema
      .array(
        tool.schema.object({
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
        })
      )
      .min(1)
      .describe("Array of ticket specifications"),
  },
  async execute(args) {
    const created: { id: string; title: string }[] = [];
    const failed: { title: string; error: string }[] = [];

    for (const ticket of args.tickets) {
      const cmdArgs: string[] = [ticket.title];

      if (ticket.description) {
        cmdArgs.push("--description", ticket.description);
      }
      if (ticket.type) {
        cmdArgs.push("--type", ticket.type);
      }
      if (ticket.priority !== undefined) {
        cmdArgs.push("--priority", String(ticket.priority));
      }
      if (ticket.parent) {
        cmdArgs.push("--parent", ticket.parent);
      }
      cmdArgs.push("--json");

      try {
        const result = await Bun.$`janus create ${cmdArgs}`.text();
        const parsed = JSON.parse(result);
        created.push({ id: parsed.id, title: ticket.title });
      } catch (e: any) {
        failed.push({
          title: ticket.title,
          error: e.stderr?.trim() || e.message || "Unknown error",
        });
      }
    }

    return { created, failed };
  },
});
