/**
 * Fern Tools Module
 *
 * This module exports all tools that Fern can use to help users.
 * Each tool is automatically registered when imported.
 *
 * To add a new tool:
 * 1. Create the tool file in src/tools/
 * 2. Export and register the tool using registerTool()
 * 3. Import the tool in this file
 */

// Import all tools to register them (side effect: registers with the tool registry)
export { RemindMeTool } from './reminders.js';
export { CheckCalendarTool, AddCalendarEventTool } from './calendar.js';
export { default as SendEmailTool } from './email.js';
export { TakeNoteTool, RecallNoteTool } from './notes.js';
export { WebSearchTool, BrowseWebTool } from './web.js';

// Re-export tool utilities from the registry
export {
  getTools,
  getTool,
  getToolsForAnthropic,
  executeTool,
  registerTool,
} from '../llm/tools.js';
