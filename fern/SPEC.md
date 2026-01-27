# Fern Implementation Spec

## Project Setup
- [x] package.json with dependencies (typescript, @anthropic-ai/sdk, twilio, better-sqlite3, drizzle-orm, express, node-cron, chrono-node, zod)
- [x] tsconfig.json configured for Node.js
- [x] src/config.ts - environment variables and settings
- [x] .env.example file

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
- [x] src/core/context.ts - User context management
- [x] src/core/engine.ts - Conversation engine with message chaining
- [x] src/core/scheduler.ts - Reminder/job scheduler

## Tools
- [x] src/tools/reminders.ts - RemindMe tool
- [x] src/tools/calendar.ts - CheckCalendar, AddCalendarEvent tools
- [x] src/tools/email.ts - SendEmail tool
- [x] src/tools/notes.ts - TakeNote, RecallNote tools
- [x] src/tools/web.ts - WebSearch, BrowseWeb tools
- [x] src/tools/index.ts - Export all tools

## Utils
- [x] src/utils/logger.ts - Logging utility
- [x] src/utils/time.ts - Timezone handling
- [x] src/utils/parsing.ts - Natural language time parsing

## Entry Point
- [x] src/index.ts - Main entry point, webhook server, startup logic

## Tests
- [x] vitest.config.ts - Test configuration
- [x] tests/setup.ts - Test setup with mock database and utilities
- [x] tests/engine.test.ts - Conversation engine tests (handleIncomingMessage, message chaining, tool execution, auth flow, locks)
- [x] tests/core.test.ts - Core component tests (context, scheduler)
- [x] tests/llm.test.ts - LLM and tool registry tests
- [x] tests/tools.test.ts - Tool execution tests (RemindMe, TakeNote, RecallNote, calendar, email, web)
- [x] tests/db.test.ts - Database layer tests
- [x] tests/messaging.test.ts - Messaging adapter tests (Twilio adapter, webhook parsing, router)
