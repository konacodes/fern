import { z } from 'zod';
import * as chrono from 'chrono-node';
import { Tool, registerTool, UserContext, ToolResult } from '../llm/tools.js';
import { db, reminders } from '../db/client.js';

// Parameters schema for RemindMe tool
const RemindMeParams = z.object({
  message: z.string().describe('What to remind the user about'),
  when: z.string().describe('When to send the reminder - natural language like "tomorrow at 5pm" or "in 2 hours"'),
});

/**
 * Parse a natural language time string into a Date
 */
function parseTime(text: string, timezone?: string): Date | null {
  const referenceDate = new Date();

  // Parse using chrono (timezone handling is done separately)
  const results = chrono.parse(text, referenceDate);

  if (results.length > 0 && results[0].date()) {
    return results[0].date();
  }

  return null;
}

/**
 * RemindMe Tool - Sets a reminder for the user at a specific time
 */
export const RemindMeTool: Tool<typeof RemindMeParams> = {
  name: 'RemindMe',
  description: 'Set a reminder for the user at a specific time. Use natural language for the time like "tomorrow at 5pm", "in 2 hours", or "next monday morning".',
  parameters: RemindMeParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { message, when } = params;

    // Parse the time
    const triggerTime = parseTime(when, userContext.timezone);

    if (!triggerTime) {
      return {
        success: false,
        error: `Could not understand the time "${when}". Try something like "tomorrow at 5pm" or "in 2 hours".`,
      };
    }

    // Check if the time is in the past
    if (triggerTime <= new Date()) {
      return {
        success: false,
        error: 'Cannot set a reminder in the past. Please specify a future time.',
      };
    }

    // Generate a unique ID for the reminder
    const id = crypto.randomUUID();

    // Store the reminder in the database
    await db.insert(reminders).values({
      id,
      userId: userContext.userId,
      message,
      triggerType: 'time',
      triggerValue: triggerTime.toISOString(),
      status: 'pending',
      createdAt: new Date(),
    });

    // Format the trigger time for confirmation
    const formattedTime = triggerTime.toLocaleString('en-US', {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      timeZone: userContext.timezone || 'UTC',
    });

    return {
      success: true,
      data: {
        reminderId: id,
        message,
        triggerTime: triggerTime.toISOString(),
        formattedTime,
      },
    };
  },
};

// Register the tool
registerTool(RemindMeTool);

export default RemindMeTool;
