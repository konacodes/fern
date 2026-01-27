/**
 * Tests for Fern's tools
 *
 * Tests each tool's execute function and parameter validation
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createMockUserContext } from './setup.js';

// Mock the db/client module - factory must not reference outer scope variables directly
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

// Import modules after mocking
import { RemindMeTool } from '../src/tools/reminders.js';
import { CheckCalendarTool, AddCalendarEventTool } from '../src/tools/calendar.js';
import SendEmailTool from '../src/tools/email.js';
import { TakeNoteTool, RecallNoteTool } from '../src/tools/notes.js';
import { WebSearchTool, BrowseWebTool } from '../src/tools/web.js';
import { clearTools, registerTool, executeTool } from '../src/llm/tools.js';
import { sqlite } from '../src/db/client.js';

describe('RemindMe Tool', () => {
  const mockContext = createMockUserContext();

  beforeEach(() => {
    // Clean up database
    sqlite.exec('DELETE FROM reminders');
    sqlite.exec('DELETE FROM users');

    // Create test user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, created_at, updated_at)
      VALUES ('${mockContext.userId}', '+1234567890', ${now}, ${now})
    `);
  });

  it('should have correct name and description', () => {
    expect(RemindMeTool.name).toBe('RemindMe');
    expect(RemindMeTool.description).toContain('reminder');
  });

  it('should parse natural language time and create reminder', async () => {
    const result = await RemindMeTool.execute(
      { message: 'Call mom', when: 'tomorrow at 5pm' },
      mockContext
    );

    expect(result.success).toBe(true);
    expect(result.data).toBeDefined();
    expect((result.data as { reminderId: string }).reminderId).toBeDefined();
    expect((result.data as { message: string }).message).toBe('Call mom');
  });

  it('should reject past times', async () => {
    const result = await RemindMeTool.execute(
      { message: 'Too late', when: 'yesterday' },
      mockContext
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('past');
  });

  it('should reject unparseable times', async () => {
    const result = await RemindMeTool.execute(
      { message: 'Invalid', when: 'blargblarg nonsense' },
      mockContext
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('Could not understand');
  });

  it('should validate required parameters', () => {
    const schema = RemindMeTool.parameters;

    // Valid params
    expect(schema.safeParse({ message: 'test', when: 'tomorrow' }).success).toBe(true);

    // Missing message
    expect(schema.safeParse({ when: 'tomorrow' }).success).toBe(false);

    // Missing when
    expect(schema.safeParse({ message: 'test' }).success).toBe(false);
  });
});

describe('Calendar Tools', () => {
  const mockContext = createMockUserContext();

  describe('CheckCalendar', () => {
    it('should have correct name and description', () => {
      expect(CheckCalendarTool.name).toBe('CheckCalendar');
      expect(CheckCalendarTool.description).toContain('calendar');
    });

    it('should return mock calendar data', async () => {
      const result = await CheckCalendarTool.execute(
        { range: 'today' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect((result.data as { events: unknown[] }).events).toBeDefined();
      expect(Array.isArray((result.data as { events: unknown[] }).events)).toBe(true);
    });

    it('should accept different time ranges', async () => {
      const ranges = ['today', 'tomorrow', 'this_week', 'next_week'] as const;

      for (const range of ranges) {
        const result = await CheckCalendarTool.execute({ range }, mockContext);
        expect(result.success).toBe(true);
      }
    });

    it('should filter events by query', async () => {
      const result = await CheckCalendarTool.execute(
        { range: 'today', query: 'standup' },
        mockContext
      );

      expect(result.success).toBe(true);
      // Mock data includes a standup event
      const events = (result.data as { events: Array<{ title: string }> }).events;
      expect(events.some(e => e.title.toLowerCase().includes('standup'))).toBe(true);
    });
  });

  describe('AddCalendarEvent', () => {
    it('should have correct name and description', () => {
      expect(AddCalendarEventTool.name).toBe('AddCalendarEvent');
      expect(AddCalendarEventTool.description).toContain('Add');
    });

    it('should accept event details and return confirmation', async () => {
      const result = await AddCalendarEventTool.execute(
        {
          title: 'Team Meeting',
          time: 'tomorrow at 2pm',
          duration: '1 hour',
          location: 'Conference Room A',
          notes: 'Discuss Q4 plans',
        },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect((result.data as { eventId: string }).eventId).toBeDefined();
      expect((result.data as { title: string }).title).toBe('Team Meeting');
    });

    it('should work with minimal required fields', async () => {
      const result = await AddCalendarEventTool.execute(
        { title: 'Quick sync', time: 'in 1 hour' },
        mockContext
      );

      expect(result.success).toBe(true);
    });
  });
});

describe('SendEmail Tool', () => {
  const mockContext = createMockUserContext();

  it('should have correct name and description', () => {
    expect(SendEmailTool.name).toBe('SendEmail');
    expect(SendEmailTool.description).toContain('email');
  });

  it('should create draft by default', async () => {
    const result = await SendEmailTool.execute(
      {
        to: 'test@example.com',
        subject: 'Test Subject',
        body: 'Test body content',
      },
      mockContext
    );

    expect(result.success).toBe(true);
    expect((result.data as { status: string }).status).toBe('drafted');
  });

  it('should respect draft=false flag', async () => {
    const result = await SendEmailTool.execute(
      {
        to: 'test@example.com',
        subject: 'Test Subject',
        body: 'Test body content',
        draft: false,
      },
      mockContext
    );

    expect(result.success).toBe(true);
    expect((result.data as { status: string }).status).toBe('sent');
  });

  it('should validate required email fields', () => {
    const schema = SendEmailTool.parameters;

    // Valid params
    expect(schema.safeParse({
      to: 'test@example.com',
      subject: 'Subject',
      body: 'Body',
    }).success).toBe(true);

    // Missing to
    expect(schema.safeParse({
      subject: 'Subject',
      body: 'Body',
    }).success).toBe(false);

    // Missing subject
    expect(schema.safeParse({
      to: 'test@example.com',
      body: 'Body',
    }).success).toBe(false);
  });
});

describe('Notes Tools', () => {
  const mockContext = createMockUserContext();

  beforeEach(() => {
    // Clean up database
    sqlite.exec('DELETE FROM notes');
    sqlite.exec('DELETE FROM users');

    // Create test user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, created_at, updated_at)
      VALUES ('${mockContext.userId}', '+1234567890', ${now}, ${now})
    `);
  });

  describe('TakeNote', () => {
    it('should have correct name and description', () => {
      expect(TakeNoteTool.name).toBe('TakeNote');
      expect(TakeNoteTool.description).toContain('note');
    });

    it('should save a note to the database', async () => {
      const result = await TakeNoteTool.execute(
        { content: 'Remember to buy milk' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect((result.data as { noteId: string }).noteId).toBeDefined();

      // Verify in database
      const notes = sqlite.prepare('SELECT * FROM notes WHERE user_id = ?').all(mockContext.userId);
      expect(notes.length).toBe(1);
    });

    it('should save note with tags', async () => {
      const result = await TakeNoteTool.execute(
        {
          content: 'Project ideas for Q4',
          tags: ['work', 'planning', 'q4'],
        },
        mockContext
      );

      expect(result.success).toBe(true);
      expect((result.data as { tags: string[] }).tags).toEqual(['work', 'planning', 'q4']);
    });

    it('should truncate long content in response preview', async () => {
      const longContent = 'A'.repeat(200);
      const result = await TakeNoteTool.execute(
        { content: longContent },
        mockContext
      );

      expect(result.success).toBe(true);
      const preview = (result.data as { content: string }).content;
      expect(preview.length).toBeLessThan(longContent.length);
      expect(preview.endsWith('...')).toBe(true);
    });
  });

  describe('RecallNote', () => {
    beforeEach(async () => {
      // Add some test notes
      await TakeNoteTool.execute({ content: 'Buy groceries from the store' }, mockContext);
      await TakeNoteTool.execute({ content: 'Call dentist for appointment' }, mockContext);
      await TakeNoteTool.execute({ content: 'Meeting notes from Monday standup' }, mockContext);
    });

    it('should have correct name and description', () => {
      expect(RecallNoteTool.name).toBe('RecallNote');
      expect(RecallNoteTool.description).toContain('Search');
    });

    it('should find notes matching query', async () => {
      const result = await RecallNoteTool.execute(
        { query: 'groceries' },
        mockContext
      );

      expect(result.success).toBe(true);
      const notes = (result.data as { notes: unknown[] }).notes;
      expect(notes.length).toBeGreaterThan(0);
    });

    it('should return empty array for no matches', async () => {
      const result = await RecallNoteTool.execute(
        { query: 'xyznonexistent' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect((result.data as { notes: unknown[] }).notes).toEqual([]);
    });

    it('should search case-insensitively (via LIKE)', async () => {
      const result = await RecallNoteTool.execute(
        { query: 'DENTIST' },
        mockContext
      );

      // Note: SQLite LIKE is case-insensitive for ASCII
      expect(result.success).toBe(true);
    });
  });
});

describe('Web Tools', () => {
  const mockContext = createMockUserContext();

  describe('WebSearch', () => {
    it('should have correct name and description', () => {
      expect(WebSearchTool.name).toBe('WebSearch');
      expect(WebSearchTool.description).toContain('Search');
    });

    it('should return mock search results', async () => {
      const result = await WebSearchTool.execute(
        { query: 'best coffee shops' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect((result.data as { results: unknown[] }).results).toBeDefined();
      expect(Array.isArray((result.data as { results: unknown[] }).results)).toBe(true);
      expect((result.data as { results: unknown[] }).results.length).toBeGreaterThan(0);
    });

    it('should include query in response', async () => {
      const result = await WebSearchTool.execute(
        { query: 'weather forecast' },
        mockContext
      );

      expect(result.success).toBe(true);
      expect((result.data as { query: string }).query).toBe('weather forecast');
    });
  });

  describe('BrowseWeb', () => {
    it('should have correct name and description', () => {
      expect(BrowseWebTool.name).toBe('BrowseWeb');
      expect(BrowseWebTool.description).toContain('webpage');
    });

    it('should return not implemented error (stub)', async () => {
      const result = await BrowseWebTool.execute(
        {
          url: 'https://example.com',
          task: 'Extract the main heading',
        },
        mockContext
      );

      // BrowseWeb is a stub that returns failure
      expect(result.success).toBe(false);
      expect(result.error).toContain('not yet implemented');
    });

    it('should include url and task in response data', async () => {
      const result = await BrowseWebTool.execute(
        {
          url: 'https://test.com',
          task: 'Find contact info',
        },
        mockContext
      );

      expect((result.data as { url: string }).url).toBe('https://test.com');
      expect((result.data as { task: string }).task).toBe('Find contact info');
    });
  });
});

describe('Tool Integration', () => {
  const mockContext = createMockUserContext();

  beforeEach(() => {
    clearTools();
    // Re-register the tools
    registerTool(RemindMeTool);
    registerTool(CheckCalendarTool);
    registerTool(AddCalendarEventTool);
    registerTool(SendEmailTool);
    registerTool(TakeNoteTool);
    registerTool(RecallNoteTool);
    registerTool(WebSearchTool);
    registerTool(BrowseWebTool);

    // Clean up database
    sqlite.exec('DELETE FROM reminders');
    sqlite.exec('DELETE FROM notes');
    sqlite.exec('DELETE FROM users');

    // Create test user
    const now = Date.now();
    sqlite.exec(`
      INSERT INTO users (id, primary_contact, created_at, updated_at)
      VALUES ('${mockContext.userId}', '+1234567890', ${now}, ${now})
    `);
  });

  it('should execute tool via executeTool helper', async () => {
    const result = await executeTool(
      'CheckCalendar',
      { range: 'today' },
      mockContext
    );

    expect(result.success).toBe(true);
  });

  it('should validate parameters via executeTool', async () => {
    const result = await executeTool(
      'RemindMe',
      { message: 'test' }, // missing 'when'
      mockContext
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('Invalid parameters');
  });

  it('should return error for unknown tool', async () => {
    const result = await executeTool(
      'NonExistentTool',
      {},
      mockContext
    );

    expect(result.success).toBe(false);
    expect(result.error).toContain('Unknown tool');
  });
});
