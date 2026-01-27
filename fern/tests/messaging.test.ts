import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  BaseAdapter,
  AdapterCapabilities,
  IncomingMessageCallback,
} from '../src/messaging/types.js';

/**
 * Mock adapter for testing
 */
class MockAdapter extends BaseAdapter {
  readonly name = 'mock';
  public sentMessages: Array<{ userId: string; content: string }> = [];
  public typingIndicators: string[] = [];

  async sendMessage(userId: string, content: string): Promise<void> {
    this.sentMessages.push({ userId, content });
  }

  async sendTypingIndicator(userId: string): Promise<void> {
    this.typingIndicators.push(userId);
  }

  getCapabilities(): AdapterCapabilities {
    return {
      typingIndicator: true,
      readReceipts: false,
      reactions: false,
      media: false,
      richCards: false,
      maxMessageLength: 1000,
    };
  }

  // Expose protected method for testing
  async simulateIncomingMessage(
    userId: string,
    content: string
  ): Promise<void> {
    await this.handleIncoming(userId, content);
  }
}

describe('MessageAdapter', () => {
  describe('BaseAdapter', () => {
    it('should register incoming message callback', async () => {
      const adapter = new MockAdapter();
      const callback = vi.fn();

      adapter.onIncomingMessage(callback);
      await adapter.simulateIncomingMessage('+1234567890', 'Hello');

      expect(callback).toHaveBeenCalledWith('+1234567890', 'Hello', undefined);
    });

    it('should not throw if no callback registered', async () => {
      const adapter = new MockAdapter();

      // Should not throw
      await expect(
        adapter.simulateIncomingMessage('+1234567890', 'Hello')
      ).resolves.toBeUndefined();
    });
  });

  describe('MockAdapter', () => {
    let adapter: MockAdapter;

    beforeEach(() => {
      adapter = new MockAdapter();
    });

    it('should have correct name', () => {
      expect(adapter.name).toBe('mock');
    });

    it('should send messages', async () => {
      await adapter.sendMessage('+1234567890', 'Test message');

      expect(adapter.sentMessages).toHaveLength(1);
      expect(adapter.sentMessages[0]).toEqual({
        userId: '+1234567890',
        content: 'Test message',
      });
    });

    it('should track typing indicators', async () => {
      await adapter.sendTypingIndicator('+1234567890');

      expect(adapter.typingIndicators).toContain('+1234567890');
    });

    it('should return capabilities', () => {
      const capabilities = adapter.getCapabilities();

      expect(capabilities.typingIndicator).toBe(true);
      expect(capabilities.media).toBe(false);
      expect(capabilities.maxMessageLength).toBe(1000);
    });
  });
});

describe('AdapterCapabilities', () => {
  it('should define all required capability fields', () => {
    const capabilities: AdapterCapabilities = {
      typingIndicator: true,
      readReceipts: true,
      reactions: true,
      media: true,
      richCards: true,
      maxMessageLength: 2000,
    };

    expect(capabilities).toHaveProperty('typingIndicator');
    expect(capabilities).toHaveProperty('readReceipts');
    expect(capabilities).toHaveProperty('reactions');
    expect(capabilities).toHaveProperty('media');
    expect(capabilities).toHaveProperty('richCards');
    expect(capabilities).toHaveProperty('maxMessageLength');
  });
});

// Note: MessageRouter tests require database integration
// These tests are in a separate integration test file
