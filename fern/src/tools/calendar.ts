import { z } from 'zod';
import { Tool, registerTool, UserContext, ToolResult } from '../llm/tools.js';

// Parameters schema for CheckCalendar tool
const CheckCalendarParams = z.object({
  range: z.enum(['today', 'tomorrow', 'this_week', 'next_week']).describe('Time range to check'),
  query: z.string().optional().describe('Optional filter for specific events'),
});

// Parameters schema for AddCalendarEvent tool
const AddCalendarEventParams = z.object({
  title: z.string().describe('Title of the event'),
  time: z.string().describe('Start time of the event in natural language'),
  duration: z.string().optional().describe('Duration like "1 hour" or "30 minutes"'),
  location: z.string().optional().describe('Location of the event'),
  notes: z.string().optional().describe('Additional notes for the event'),
});

// Mock calendar data for stub implementation
const mockEvents = [
  { id: '1', title: 'Team standup', time: '10:00 AM', duration: '30 min' },
  { id: '2', title: 'Lunch with Sarah', time: '12:00 PM', duration: '1 hour' },
  { id: '3', title: 'Dentist appointment', time: '3:00 PM', duration: '1 hour' },
  { id: '4', title: 'Project review', time: '4:30 PM', duration: '45 min' },
];

/**
 * CheckCalendar Tool - Look at the user's calendar for availability or events
 *
 * NOTE: This is a stub implementation that returns mock data.
 * Future implementation will integrate with Google Calendar, Apple Calendar, etc.
 */
export const CheckCalendarTool: Tool<typeof CheckCalendarParams> = {
  name: 'CheckCalendar',
  description: 'Look at the user\'s calendar for availability or events. Returns a list of events for the specified time range.',
  parameters: CheckCalendarParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { range, query } = params;

    console.log(`[Calendar] Checking calendar for ${userContext.userId}:`, { range, query });

    // Filter mock events if query is provided
    let events = [...mockEvents];
    if (query) {
      const lowerQuery = query.toLowerCase();
      events = events.filter(e =>
        e.title.toLowerCase().includes(lowerQuery)
      );
    }

    // Format the range for the response
    const rangeLabels: Record<string, string> = {
      today: 'today',
      tomorrow: 'tomorrow',
      this_week: 'this week',
      next_week: 'next week',
    };

    return {
      success: true,
      data: {
        range: rangeLabels[range] || range,
        events,
        note: 'This is mock data. Calendar integration coming soon!',
      },
    };
  },
};

/**
 * AddCalendarEvent Tool - Add an event to the user's calendar
 *
 * NOTE: This is a stub implementation that logs the event.
 * Future implementation will integrate with Google Calendar, Apple Calendar, etc.
 */
export const AddCalendarEventTool: Tool<typeof AddCalendarEventParams> = {
  name: 'AddCalendarEvent',
  description: 'Add an event to the user\'s calendar. Specify the title, time, and optionally duration, location, and notes.',
  parameters: AddCalendarEventParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { title, time, duration, location, notes } = params;

    console.log(`[Calendar] Adding event for ${userContext.userId}:`, {
      title,
      time,
      duration,
      location,
      notes,
    });

    // Generate a mock event ID
    const eventId = `mock-${Date.now()}`;

    return {
      success: true,
      data: {
        eventId,
        title,
        time,
        duration: duration || '1 hour',
        location,
        notes,
        note: 'Event logged (stub). Calendar integration coming soon!',
      },
    };
  },
};

// Register the tools
registerTool(CheckCalendarTool);
registerTool(AddCalendarEventTool);
