/**
 * Test Setup - Common test configuration and mocks
 *
 * This module provides:
 * - In-memory SQLite database for tests
 * - Mock Anthropic API
 * - Mock Twilio SDK
 * - Utility functions for test data
 */

import { vi } from 'vitest';
import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import * as schema from '../src/db/schema.js';

/**
 * Create an in-memory SQLite database for testing
 */
export function createTestDatabase() {
  const sqlite = new Database(':memory:');
  sqlite.pragma('journal_mode = WAL');

  // Create tables matching the schema
  sqlite.exec(`
    CREATE TABLE users (
      id TEXT PRIMARY KEY,
      primary_contact TEXT NOT NULL,
      name TEXT,
      timezone TEXT,
      preferences TEXT,
      knowledge TEXT,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );

    CREATE TABLE conversations (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id),
      adapter_type TEXT NOT NULL,
      started_at INTEGER NOT NULL,
      last_message_at INTEGER NOT NULL
    );

    CREATE TABLE messages (
      id TEXT PRIMARY KEY,
      conversation_id TEXT NOT NULL REFERENCES conversations(id),
      role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
      content TEXT NOT NULL,
      metadata TEXT,
      created_at INTEGER NOT NULL
    );

    CREATE TABLE reminders (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id),
      message TEXT NOT NULL,
      trigger_type TEXT NOT NULL CHECK(trigger_type IN ('time', 'condition')),
      trigger_value TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'delivered', 'cancelled')),
      created_at INTEGER NOT NULL,
      triggered_at INTEGER
    );

    CREATE TABLE notes (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id),
      content TEXT NOT NULL,
      tags TEXT,
      created_at INTEGER NOT NULL
    );
  `);

  const db = drizzle(sqlite, { schema });

  return { db, sqlite };
}

/**
 * Clean up all tables in the test database
 */
export function cleanupTables(sqlite: Database.Database) {
  sqlite.exec('DELETE FROM messages');
  sqlite.exec('DELETE FROM conversations');
  sqlite.exec('DELETE FROM reminders');
  sqlite.exec('DELETE FROM notes');
  sqlite.exec('DELETE FROM users');
}

/**
 * Create a test user in the database
 */
export function createTestUser(sqlite: Database.Database, userId: string, contact: string = '+1234567890') {
  const now = Date.now();
  sqlite.exec(`
    INSERT INTO users (id, primary_contact, name, timezone, created_at, updated_at)
    VALUES ('${userId}', '${contact}', 'Test User', 'America/New_York', ${now}, ${now})
  `);
}

/**
 * Mock Anthropic API responses
 */
export function createMockAnthropicClient() {
  return {
    messages: {
      create: vi.fn().mockResolvedValue({
        id: 'msg_mock',
        type: 'message',
        role: 'assistant',
        content: [{ type: 'text', text: 'Mock response' }],
        model: 'claude-3-haiku-20240307',
        stop_reason: 'end_turn',
        usage: { input_tokens: 10, output_tokens: 5 },
      }),
      stream: vi.fn().mockReturnValue({
        async *[Symbol.asyncIterator]() {
          yield {
            type: 'content_block_delta',
            index: 0,
            delta: { type: 'text_delta', text: 'Mock ' },
          };
          yield {
            type: 'content_block_delta',
            index: 0,
            delta: { type: 'text_delta', text: 'response' },
          };
          yield {
            type: 'message_stop',
          };
        },
      }),
    },
  };
}

/**
 * Mock Twilio client
 */
export function createMockTwilioClient() {
  const mockMessages = {
    create: vi.fn().mockResolvedValue({
      sid: 'SM_mock',
      status: 'sent',
      to: '+1234567890',
      from: '+0987654321',
    }),
  };

  return {
    messages: mockMessages,
  };
}

/**
 * Mock user context for tool testing
 */
export function createMockUserContext(overrides: Record<string, unknown> = {}) {
  return {
    userId: 'test-user-123',
    name: 'Test User',
    timezone: 'America/New_York',
    preferences: {},
    knowledge: {},
    ...overrides,
  };
}

/**
 * Mock message adapter for testing
 */
export function createMockMessageAdapter() {
  const sentMessages: Array<{ userId: string; content: string }> = [];
  const typingIndicators: string[] = [];

  return {
    name: 'mock',
    sentMessages,
    typingIndicators,
    onIncomingMessage: vi.fn(),
    sendMessage: vi.fn(async (userId: string, content: string) => {
      sentMessages.push({ userId, content });
    }),
    sendTypingIndicator: vi.fn(async (userId: string) => {
      typingIndicators.push(userId);
    }),
    getCapabilities: () => ({
      typingIndicator: true,
      readReceipts: false,
      reactions: false,
      media: false,
      richCards: false,
      maxMessageLength: 1600,
    }),
  };
}

/**
 * Wait for async operations to complete
 */
export function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
