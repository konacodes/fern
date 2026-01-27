import { sqliteTable, text, integer } from 'drizzle-orm/sqlite-core';

// Users table - stores user identity, preferences, and learned knowledge
export const users = sqliteTable('users', {
  id: text('id').primaryKey(),
  primaryContact: text('primary_contact').notNull(),
  name: text('name'),
  timezone: text('timezone'),
  preferences: text('preferences'), // JSON string
  knowledge: text('knowledge'), // JSON string
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
  updatedAt: integer('updated_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
});

// Conversations table - groups messages by session/adapter
export const conversations = sqliteTable('conversations', {
  id: text('id').primaryKey(),
  userId: text('user_id').notNull().references(() => users.id),
  adapterType: text('adapter_type').notNull(), // 'twilio', 'bluebubbles', 'mock'
  startedAt: integer('started_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
  lastMessageAt: integer('last_message_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
});

// Messages table - individual messages in a conversation
export const messages = sqliteTable('messages', {
  id: text('id').primaryKey(),
  conversationId: text('conversation_id').notNull().references(() => conversations.id),
  role: text('role', { enum: ['user', 'assistant'] }).notNull(),
  content: text('content').notNull(),
  metadata: text('metadata'), // JSON string for tool calls, etc.
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
});

// Reminders table - scheduled notifications
export const reminders = sqliteTable('reminders', {
  id: text('id').primaryKey(),
  userId: text('user_id').notNull().references(() => users.id),
  message: text('message').notNull(),
  triggerType: text('trigger_type', { enum: ['time', 'condition'] }).notNull(),
  triggerValue: text('trigger_value').notNull(), // ISO timestamp or condition string
  status: text('status', { enum: ['pending', 'delivered', 'cancelled'] }).notNull().default('pending'),
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
  triggeredAt: integer('triggered_at', { mode: 'timestamp' }),
});

// Notes table - user's saved notes
export const notes = sqliteTable('notes', {
  id: text('id').primaryKey(),
  userId: text('user_id').notNull().references(() => users.id),
  content: text('content').notNull(),
  tags: text('tags'), // JSON array string
  createdAt: integer('created_at', { mode: 'timestamp' }).notNull().$defaultFn(() => new Date()),
});

// Type exports for use in application code
export type User = typeof users.$inferSelect;
export type NewUser = typeof users.$inferInsert;

export type Conversation = typeof conversations.$inferSelect;
export type NewConversation = typeof conversations.$inferInsert;

export type Message = typeof messages.$inferSelect;
export type NewMessage = typeof messages.$inferInsert;

export type Reminder = typeof reminders.$inferSelect;
export type NewReminder = typeof reminders.$inferInsert;

export type Note = typeof notes.$inferSelect;
export type NewNote = typeof notes.$inferInsert;
