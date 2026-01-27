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

describe('TwilioAdapter', () => {
  it('should have name "twilio"', async () => {
    // Import with mocked Twilio
    const { TwilioAdapter } = await import('../src/messaging/twilio.js');

    const adapter = new TwilioAdapter({
      accountSid: 'ACtest123',
      authToken: 'test-auth-token',
      phoneNumber: '+15551234567',
    });

    expect(adapter.name).toBe('twilio');
  });

  it('should return correct capabilities', async () => {
    const { TwilioAdapter } = await import('../src/messaging/twilio.js');

    const adapter = new TwilioAdapter({
      accountSid: 'ACtest123',
      authToken: 'test-auth-token',
      phoneNumber: '+15551234567',
    });

    const capabilities = adapter.getCapabilities();

    expect(capabilities.typingIndicator).toBe(false); // SMS doesn't support
    expect(capabilities.readReceipts).toBe(false);
    expect(capabilities.media).toBe(true); // MMS supports media
    expect(capabilities.maxMessageLength).toBe(1600);
  });

  it('should create an Express router for webhooks', async () => {
    const { TwilioAdapter } = await import('../src/messaging/twilio.js');

    const adapter = new TwilioAdapter({
      accountSid: 'ACtest123',
      authToken: 'test-auth-token',
      phoneNumber: '+15551234567',
    });

    const router = adapter.getRouter();
    expect(router).toBeDefined();
  });

  it('sendTypingIndicator should be no-op for SMS', async () => {
    const { TwilioAdapter } = await import('../src/messaging/twilio.js');

    const adapter = new TwilioAdapter({
      accountSid: 'ACtest123',
      authToken: 'test-auth-token',
      phoneNumber: '+15551234567',
    });

    // Should not throw
    await expect(adapter.sendTypingIndicator('+15559876543')).resolves.toBeUndefined();
  });
});

describe('Webhook Parsing', () => {
  it('should parse incoming SMS webhook body', () => {
    // Simulate Twilio webhook body structure
    const webhookBody = {
      From: '+15559876543',
      To: '+15551234567',
      Body: 'Hello Fern!',
      MessageSid: 'SM1234567890',
      NumMedia: '0',
    };

    // Extract message data like the adapter does
    const userId = webhookBody.From;
    const content = webhookBody.Body || '';
    const messageId = webhookBody.MessageSid;
    const numMedia = parseInt(webhookBody.NumMedia || '0', 10);

    expect(userId).toBe('+15559876543');
    expect(content).toBe('Hello Fern!');
    expect(messageId).toBe('SM1234567890');
    expect(numMedia).toBe(0);
  });

  it('should parse webhook with media attachments', () => {
    const webhookBody = {
      From: '+15559876543',
      To: '+15551234567',
      Body: 'Check this out!',
      MessageSid: 'SM1234567890',
      NumMedia: '2',
      MediaUrl0: 'https://api.twilio.com/media/image1.jpg',
      MediaContentType0: 'image/jpeg',
      MediaUrl1: 'https://api.twilio.com/media/file.pdf',
      MediaContentType1: 'application/pdf',
    };

    const numMedia = parseInt(webhookBody.NumMedia || '0', 10);
    expect(numMedia).toBe(2);

    // Extract media like the adapter does
    const media = [];
    for (let i = 0; i < numMedia; i++) {
      const mediaUrl = (webhookBody as Record<string, string>)[`MediaUrl${i}`];
      const mediaType = (webhookBody as Record<string, string>)[`MediaContentType${i}`];
      if (mediaUrl) {
        media.push({ url: mediaUrl, mimeType: mediaType });
      }
    }

    expect(media.length).toBe(2);
    expect(media[0].url).toContain('image1.jpg');
    expect(media[0].mimeType).toBe('image/jpeg');
    expect(media[1].mimeType).toBe('application/pdf');
  });
});

// Note: MessageRouter tests require database integration
// These tests are in a separate integration test file (core.test.ts)
