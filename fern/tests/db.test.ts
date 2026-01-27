import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest';
import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import { migrate } from 'drizzle-orm/better-sqlite3/migrator';
import { eq } from 'drizzle-orm';
import * as schema from '../src/db/schema.js';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

describe('Database Schema', () => {
  let sqlite: Database.Database;
  let db: ReturnType<typeof drizzle>;

  beforeAll(() => {
    // Use in-memory database for tests
    sqlite = new Database(':memory:');
    db = drizzle(sqlite, { schema });

    // Run migrations
    const migrationsFolder = resolve(__dirname, '../drizzle');
    migrate(db, { migrationsFolder });
  });

  afterAll(() => {
    sqlite.close();
  });

  beforeEach(() => {
    // Clear tables before each test
    sqlite.exec('DELETE FROM notes');
    sqlite.exec('DELETE FROM reminders');
    sqlite.exec('DELETE FROM messages');
    sqlite.exec('DELETE FROM conversations');
    sqlite.exec('DELETE FROM users');
  });

  describe('users table', () => {
    it('should create a user with required fields', () => {
      const user: schema.NewUser = {
        id: 'user-1',
        primaryContact: '+15551234567',
      };

      db.insert(schema.users).values(user).run();

      const result = db.select().from(schema.users).where(eq(schema.users.id, 'user-1')).get();
      expect(result).toBeDefined();
      expect(result?.primaryContact).toBe('+15551234567');
      expect(result?.createdAt).toBeInstanceOf(Date);
    });

    it('should create a user with all fields', () => {
      const preferences = JSON.stringify({ communicationStyle: 'casual' });
      const knowledge = JSON.stringify({ favoriteCoffee: 'oat milk latte' });

      const user: schema.NewUser = {
        id: 'user-2',
        primaryContact: 'user@icloud.com',
        name: 'Test User',
        timezone: 'America/New_York',
        preferences,
        knowledge,
      };

      db.insert(schema.users).values(user).run();

      const result = db.select().from(schema.users).where(eq(schema.users.id, 'user-2')).get();
      expect(result?.name).toBe('Test User');
      expect(result?.timezone).toBe('America/New_York');
      expect(JSON.parse(result?.preferences || '{}')).toEqual({ communicationStyle: 'casual' });
      expect(JSON.parse(result?.knowledge || '{}')).toEqual({ favoriteCoffee: 'oat milk latte' });
    });
  });

  describe('conversations table', () => {
    it('should create a conversation linked to a user', () => {
      // First create a user
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();

      // Create conversation
      const conversation: schema.NewConversation = {
        id: 'conv-1',
        userId: 'user-1',
        adapterType: 'twilio',
      };

      db.insert(schema.conversations).values(conversation).run();

      const result = db.select().from(schema.conversations).where(eq(schema.conversations.id, 'conv-1')).get();
      expect(result?.userId).toBe('user-1');
      expect(result?.adapterType).toBe('twilio');
      expect(result?.startedAt).toBeInstanceOf(Date);
    });
  });

  describe('messages table', () => {
    it('should create messages with different roles', () => {
      // Setup
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();
      db.insert(schema.conversations).values({ id: 'conv-1', userId: 'user-1', adapterType: 'twilio' }).run();

      // Create user message
      db.insert(schema.messages).values({
        id: 'msg-1',
        conversationId: 'conv-1',
        role: 'user',
        content: 'remind me to call mom',
      }).run();

      // Create assistant message
      db.insert(schema.messages).values({
        id: 'msg-2',
        conversationId: 'conv-1',
        role: 'assistant',
        content: 'on it!',
      }).run();

      const messages = db.select().from(schema.messages).all();
      expect(messages).toHaveLength(2);
      expect(messages[0].role).toBe('user');
      expect(messages[1].role).toBe('assistant');
    });

    it('should store metadata as JSON string', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();
      db.insert(schema.conversations).values({ id: 'conv-1', userId: 'user-1', adapterType: 'twilio' }).run();

      const metadata = JSON.stringify({
        toolCalls: [{ name: 'RemindMe', input: { message: 'call mom' } }],
      });

      db.insert(schema.messages).values({
        id: 'msg-1',
        conversationId: 'conv-1',
        role: 'assistant',
        content: 'Setting reminder...',
        metadata,
      }).run();

      const result = db.select().from(schema.messages).where(eq(schema.messages.id, 'msg-1')).get();
      const parsedMetadata = JSON.parse(result?.metadata || '{}');
      expect(parsedMetadata.toolCalls[0].name).toBe('RemindMe');
    });
  });

  describe('reminders table', () => {
    it('should create a time-based reminder', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();

      const reminder: schema.NewReminder = {
        id: 'rem-1',
        userId: 'user-1',
        message: 'Call mom',
        triggerType: 'time',
        triggerValue: '2025-01-15T17:00:00Z',
      };

      db.insert(schema.reminders).values(reminder).run();

      const result = db.select().from(schema.reminders).where(eq(schema.reminders.id, 'rem-1')).get();
      expect(result?.triggerType).toBe('time');
      expect(result?.status).toBe('pending');
    });

    it('should create a condition-based reminder', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();

      const reminder: schema.NewReminder = {
        id: 'rem-2',
        userId: 'user-1',
        message: 'Buy groceries',
        triggerType: 'condition',
        triggerValue: 'when I get home',
      };

      db.insert(schema.reminders).values(reminder).run();

      const result = db.select().from(schema.reminders).where(eq(schema.reminders.id, 'rem-2')).get();
      expect(result?.triggerType).toBe('condition');
      expect(result?.triggerValue).toBe('when I get home');
    });

    it('should update reminder status', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();
      db.insert(schema.reminders).values({
        id: 'rem-1',
        userId: 'user-1',
        message: 'Test',
        triggerType: 'time',
        triggerValue: '2025-01-15T17:00:00Z',
      }).run();

      db.update(schema.reminders)
        .set({ status: 'delivered', triggeredAt: new Date() })
        .where(eq(schema.reminders.id, 'rem-1'))
        .run();

      const result = db.select().from(schema.reminders).where(eq(schema.reminders.id, 'rem-1')).get();
      expect(result?.status).toBe('delivered');
      expect(result?.triggeredAt).toBeInstanceOf(Date);
    });
  });

  describe('notes table', () => {
    it('should create a note with tags', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();

      const tags = JSON.stringify(['shopping', 'groceries']);
      const note: schema.NewNote = {
        id: 'note-1',
        userId: 'user-1',
        content: 'Milk, eggs, bread',
        tags,
      };

      db.insert(schema.notes).values(note).run();

      const result = db.select().from(schema.notes).where(eq(schema.notes.id, 'note-1')).get();
      expect(result?.content).toBe('Milk, eggs, bread');
      expect(JSON.parse(result?.tags || '[]')).toEqual(['shopping', 'groceries']);
    });

    it('should create a note without tags', () => {
      db.insert(schema.users).values({ id: 'user-1', primaryContact: '+15551234567' }).run();

      db.insert(schema.notes).values({
        id: 'note-2',
        userId: 'user-1',
        content: 'Random thought',
      }).run();

      const result = db.select().from(schema.notes).where(eq(schema.notes.id, 'note-2')).get();
      expect(result?.content).toBe('Random thought');
      expect(result?.tags).toBeNull();
    });
  });
});
