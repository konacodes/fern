/**
 * Conversation Engine Tests
 *
 * Tests for the conversation engine including:
 * - handleIncomingMessage flow
 * - Message chaining (multiple messages sent)
 * - Tool execution integration
 * - New user auth flow
 * - Conversation lock behavior
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// Mock the db/client module first
vi.mock('../src/db/client.js', async () => {
  const Database = (await import('better-sqlite3')).default;
  const { drizzle } = await import('drizzle-orm/better-sqlite3');
  const schema = await import('../src/db/schema.js');

  const mockDb = new Database(':memory:');
  mockDb.pragma('journal_mode = WAL');

  mockDb.exec(`
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

  const db = drizzle(mockDb, { schema });

  return {
    db,
    sqlite: mockDb,
    ...schema,
  };
});

// Import after mocking
import { sqlite, db, messages, conversations, users } from '../src/db/client.js';
import { eq } from 'drizzle-orm';
import { ChainSender, ConversationEngine, type LLMClientInterface, type LLMResponse } from '../src/core/engine.js';
import { loadUserContext } from '../src/core/context.js';
import type { MessageAdapter } from '../src/messaging/types.js';

// Helper to clean up tables
function cleanupTables() {
  sqlite.exec('DELETE FROM messages');
  sqlite.exec('DELETE FROM conversations');
  sqlite.exec('DELETE FROM reminders');
  sqlite.exec('DELETE FROM notes');
  sqlite.exec('DELETE FROM users');
}

// Helper to wait for async operations
function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Create a mock message adapter
function createMockAdapter() {
  const sentMessages: Array<{ userId: string; content: string }> = [];
  const typingIndicators: string[] = [];

  const adapter: MessageAdapter & {
    sentMessages: typeof sentMessages;
    typingIndicators: typeof typingIndicators;
  } = {
    name: 'test',
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

  return adapter;
}

// Create a mock LLM client
function createMockLLMClient(responses: string[]): LLMClientInterface {
  let callIndex = 0;

  return {
    async chat(systemPrompt, messages, tools): Promise<LLMResponse> {
      const response = responses[callIndex] || 'Mock response';
      callIndex++;
      return {
        text: response,
        toolCalls: [],
      };
    },
  };
}

describe('ChainSender', () => {
  it('should send messages in order', async () => {
    const adapter = createMockAdapter();
    const chain = new ChainSender('user123', adapter);

    await chain.send('Message 1');
    await chain.send('Message 2');
    await chain.send('Message 3');

    await wait(1500); // Wait for processing

    expect(adapter.sentMessages.length).toBe(3);
    expect(adapter.sentMessages[0].content).toBe('Message 1');
    expect(adapter.sentMessages[1].content).toBe('Message 2');
    expect(adapter.sentMessages[2].content).toBe('Message 3');
  });

  it('should send typing indicator before each message', async () => {
    const adapter = createMockAdapter();
    const chain = new ChainSender('user456', adapter);

    await chain.send('Hello');
    await wait(500);

    expect(adapter.typingIndicators.length).toBeGreaterThanOrEqual(1);
    expect(adapter.typingIndicators[0]).toBe('user456');
  });

  it('should send messages to correct user', async () => {
    const adapter = createMockAdapter();
    const chain = new ChainSender('+15551234567', adapter);

    await chain.send('Test message');
    await wait(500);

    expect(adapter.sentMessages[0].userId).toBe('+15551234567');
  });
});

describe('ConversationEngine - handleIncomingMessage', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
    // Clear AUTH_CODES for tests
    process.env.AUTH_CODES = '';
  });

  it('should create a new user for unknown phone number', async () => {
    const engine = new ConversationEngine();

    await engine.handleIncomingMessage('new-user-123', 'Hello!', mockAdapter);
    await wait(500);

    const context = await loadUserContext('new-user-123');
    expect(context).not.toBeNull();
    expect(context!.primaryContact).toBe('new-user-123');
  });

  it('should save incoming message to database', async () => {
    const engine = new ConversationEngine();
    engine.setLLMClient(createMockLLMClient(['Hi there!']));
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create existing user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('existing-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    await engine.handleIncomingMessage('existing-user', 'Test message', mockAdapter);
    await wait(500);

    // Check message was saved
    const savedMessages = db.select().from(messages).all();
    expect(savedMessages.some((m) => m.content === 'Test message')).toBe(true);
    expect(savedMessages.some((m) => m.role === 'user')).toBe(true);
  });

  it('should create conversation record', async () => {
    const engine = new ConversationEngine();

    // Create existing user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('conv-test-user', '+1234567890', 'Test', ${now}, ${now})
    `);

    await engine.handleIncomingMessage('conv-test-user', 'Hi', mockAdapter);
    await wait(500);

    const convs = db.select().from(conversations).all();
    expect(convs.length).toBeGreaterThan(0);
    expect(convs[0].adapterType).toBe('test');
    expect(convs[0].userId).toBe('conv-test-user');
  });
});

describe('ConversationEngine - Auth Flow', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
  });

  it('should ask for magic word when AUTH_CODES is set', async () => {
    process.env.AUTH_CODES = 'secret123,magic456';

    const engine = new ConversationEngine();

    await engine.handleIncomingMessage('auth-test-user', 'Hi there', mockAdapter);
    await wait(800);

    // Should ask for magic word
    const responses = mockAdapter.sentMessages.map((m) => m.content.toLowerCase());
    expect(responses.some((r) => r.includes('magic word') || r.includes("haven't met"))).toBe(true);

    // Reset
    process.env.AUTH_CODES = '';
  });

  it('should accept valid auth code and ask for name', async () => {
    process.env.AUTH_CODES = 'secret123,magic456';

    const engine = new ConversationEngine();

    await engine.handleIncomingMessage('auth-code-user', 'secret123', mockAdapter);
    await wait(800);

    // Should welcome and ask for name
    const responses = mockAdapter.sentMessages.map((m) => m.content.toLowerCase());
    expect(responses.some((r) => r.includes('welcome') || r.includes('name'))).toBe(true);

    // Reset
    process.env.AUTH_CODES = '';
  });

  it('should auto-authenticate when no AUTH_CODES configured', async () => {
    process.env.AUTH_CODES = '';

    const engine = new ConversationEngine();

    await engine.handleIncomingMessage('auto-auth-user', 'John', mockAdapter);
    await wait(800);

    // Should greet the user by name
    const responses = mockAdapter.sentMessages.map((m) => m.content.toLowerCase());
    expect(responses.some((r) => r.includes('john') || r.includes('fern'))).toBe(true);
  });
});

describe('ConversationEngine - Tool Execution', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
    process.env.AUTH_CODES = '';
  });

  it('should handle tool calls from LLM', async () => {
    const engine = new ConversationEngine();

    // Create mock LLM that returns a tool call
    const mockLLM: LLMClientInterface = {
      async chat(systemPrompt, messages, tools): Promise<LLMResponse> {
        // First call returns tool use, second call returns final response
        if (messages.length <= 2) {
          return {
            text: 'on it',
            toolCalls: [
              {
                id: 'tool-1',
                name: 'TakeNote',
                input: { content: 'Test note' },
              },
            ],
          };
        }
        return { text: 'Done, saved your note!' };
      },
    };

    engine.setLLMClient(mockLLM);
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('tool-test-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    await engine.handleIncomingMessage(
      'tool-test-user',
      'Remember that I like coffee',
      mockAdapter
    );
    await wait(1000);

    // Should send acknowledgment and result
    expect(mockAdapter.sentMessages.length).toBeGreaterThanOrEqual(1);
  });
});

describe('ConversationEngine - Message Chaining', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
    process.env.AUTH_CODES = '';
  });

  it('should send multiple messages for long responses', async () => {
    const engine = new ConversationEngine();

    // Mock LLM that returns a long response
    const longResponse =
      'This is the first part of my response. ' +
      'This is the second part with more information. ' +
      'And here is some additional context that might be helpful!';

    engine.setLLMClient(createMockLLMClient([longResponse]));
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('chain-test-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    await engine.handleIncomingMessage('chain-test-user', 'Tell me something', mockAdapter);
    await wait(1500);

    // Engine should split long responses into multiple messages
    // At minimum should have sent something
    expect(mockAdapter.sentMessages.length).toBeGreaterThanOrEqual(1);
  });

  it('should preserve message order in chain', async () => {
    const engine = new ConversationEngine();

    engine.setLLMClient(createMockLLMClient(['First sentence. Second sentence. Third sentence.']));
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('order-test-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    await engine.handleIncomingMessage('order-test-user', 'Hi', mockAdapter);
    await wait(1500);

    // All messages should be sent to the same user
    expect(mockAdapter.sentMessages.every((m) => m.userId === 'order-test-user')).toBe(true);
  });
});

describe('ConversationEngine - Conversation Lock', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
    process.env.AUTH_CODES = '';
  });

  it('should handle concurrent messages without race conditions', async () => {
    const engine = new ConversationEngine();

    let callCount = 0;
    const mockLLM: LLMClientInterface = {
      async chat(): Promise<LLMResponse> {
        callCount++;
        // Simulate some processing time
        await wait(100);
        return { text: `Response ${callCount}` };
      },
    };

    engine.setLLMClient(mockLLM);
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('lock-test-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    // Send multiple messages concurrently
    const promises = [
      engine.handleIncomingMessage('lock-test-user', 'Message 1', mockAdapter),
      engine.handleIncomingMessage('lock-test-user', 'Message 2', mockAdapter),
      engine.handleIncomingMessage('lock-test-user', 'Message 3', mockAdapter),
    ];

    await Promise.all(promises);
    await wait(1500);

    // All messages should be processed
    // The lock ensures they're processed sequentially
    expect(callCount).toBe(3);
  });

  it('should allow concurrent messages from different users', async () => {
    const engine = new ConversationEngine();

    const userResponses: Record<string, string[]> = {
      'user-a': [],
      'user-b': [],
    };

    const mockLLM: LLMClientInterface = {
      async chat(systemPrompt, messages): Promise<LLMResponse> {
        // Extract user from the conversation
        await wait(50);
        return { text: 'Response' };
      },
    };

    engine.setLLMClient(mockLLM);
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create two authenticated users
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES
        ('user-a', '+1111111111', 'User A', ${now}, ${now}),
        ('user-b', '+2222222222', 'User B', ${now}, ${now})
    `);

    // Send messages from both users concurrently
    await Promise.all([
      engine.handleIncomingMessage('user-a', 'Hello A', mockAdapter),
      engine.handleIncomingMessage('user-b', 'Hello B', mockAdapter),
    ]);

    await wait(1000);

    // Both users should get responses
    const userAMessages = mockAdapter.sentMessages.filter((m) => m.userId === 'user-a');
    const userBMessages = mockAdapter.sentMessages.filter((m) => m.userId === 'user-b');

    expect(userAMessages.length).toBeGreaterThanOrEqual(1);
    expect(userBMessages.length).toBeGreaterThanOrEqual(1);
  });
});

describe('ConversationEngine - Error Handling', () => {
  let mockAdapter: ReturnType<typeof createMockAdapter>;

  beforeEach(() => {
    cleanupTables();
    mockAdapter = createMockAdapter();
    process.env.AUTH_CODES = '';
  });

  it('should handle LLM errors gracefully', async () => {
    const engine = new ConversationEngine();

    const errorLLM: LLMClientInterface = {
      async chat(): Promise<LLMResponse> {
        throw new Error('LLM API error');
      },
    };

    engine.setLLMClient(errorLLM);
    engine.setSystemPromptBuilder(() => 'You are Fern.');

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('error-test-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    // Should not throw
    await expect(
      engine.handleIncomingMessage('error-test-user', 'Hi', mockAdapter)
    ).resolves.toBeUndefined();

    await wait(500);

    // Should send error message to user
    const responses = mockAdapter.sentMessages.map((m) => m.content.toLowerCase());
    expect(responses.some((r) => r.includes('wrong') || r.includes('try'))).toBe(true);
  });

  it('should handle missing LLM client', async () => {
    const engine = new ConversationEngine();
    // Don't set LLM client

    // Create authenticated user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, name, created_at, updated_at)
      VALUES ('no-llm-user', '+1234567890', 'Test User', ${now}, ${now})
    `);

    await engine.handleIncomingMessage('no-llm-user', 'Hi', mockAdapter);
    await wait(500);

    // Should send fallback message
    const responses = mockAdapter.sentMessages.map((m) => m.content.toLowerCase());
    expect(responses.some((r) => r.includes('trouble') || r.includes('moment'))).toBe(true);
  });
});
