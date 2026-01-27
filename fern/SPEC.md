# Fern Implementation Spec

## Project Setup
- [ ] package.json with dependencies (typescript, @anthropic-ai/sdk, twilio, better-sqlite3, drizzle-orm, express, node-cron, chrono-node, zod)
- [ ] tsconfig.json configured for Node.js
- [ ] src/config.ts - environment variables and settings
- [ ] .env.example file

## Database Layer
- [x] src/db/schema.ts - Drizzle schema (users, conversations, messages, reminders, notes)
- [x] src/db/client.ts - Database connection
- [x] src/db/migrations/ - Initial migration

## Messaging Layer
- [x] src/messaging/types.ts - MessageAdapter interface
- [x] src/messaging/twilio.ts - Twilio SMS adapter
- [x] src/messaging/router.ts - Routes messages to correct adapter

## LLM Layer
- [x] src/llm/client.ts - Anthropic client wrapper with streaming
- [x] src/llm/prompts.ts - System prompts and Fern personality
- [x] src/llm/tools.ts - Tool definitions registry

## Core Engine
- [ ] src/core/context.ts - User context management
- [ ] src/core/engine.ts - Conversation engine with message chaining
- [ ] src/core/scheduler.ts - Reminder/job scheduler

## Tools
- [x] src/tools/reminders.ts - RemindMe tool
- [x] src/tools/calendar.ts - CheckCalendar, AddCalendarEvent tools
- [x] src/tools/email.ts - SendEmail tool
- [x] src/tools/notes.ts - TakeNote, RecallNote tools
- [x] src/tools/web.ts - WebSearch, BrowseWeb tools
- [x] src/tools/index.ts - Export all tools

## Utils
- [ ] src/utils/logger.ts - Logging utility
- [ ] src/utils/time.ts - Timezone handling
- [ ] src/utils/parsing.ts - Natural language time parsing

## Entry Point
- [ ] src/index.ts - Main entry point, webhook server, startup logic

## Tests
- [ ] tests/setup.ts - Test configuration
- [ ] tests/engine.test.ts - Conversation engine tests
- [ ] tests/tools.test.ts - Tool execution tests
- [x] tests/messaging.test.ts - Messaging adapter tests
