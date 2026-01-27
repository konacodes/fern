/**
 * Message router for Fern
 *
 * Routes messages to the correct adapter based on the user's conversation history.
 * Each user/conversation is associated with the adapter they originally contacted Fern through.
 */

import { eq, desc } from 'drizzle-orm';
import { db, conversations } from '../db/client.js';
import { MessageAdapter, IncomingMessageCallback } from './types.js';

/**
 * Message router configuration
 */
export interface MessageRouterConfig {
  /** Default adapter to use for new conversations */
  defaultAdapter?: string;
}

/**
 * Routes messages to the appropriate adapter
 */
export class MessageRouter {
  private adapters: Map<string, MessageAdapter> = new Map();
  private config: MessageRouterConfig;
  private messageCallback: IncomingMessageCallback | null = null;

  constructor(config: MessageRouterConfig = {}) {
    this.config = config;
  }

  /**
   * Register an adapter with the router
   */
  registerAdapter(adapter: MessageAdapter): void {
    this.adapters.set(adapter.name, adapter);

    // Forward incoming messages from this adapter to the router's callback
    adapter.onIncomingMessage(async (userId, content, metadata) => {
      // Track which adapter this user is using
      await this.trackUserAdapter(userId, adapter.name);

      // Forward to the main message handler
      if (this.messageCallback) {
        await this.messageCallback(userId, content, metadata);
      }
    });
  }

  /**
   * Get a registered adapter by name
   */
  getAdapter(name: string): MessageAdapter | undefined {
    return this.adapters.get(name);
  }

  /**
   * Get all registered adapters
   */
  getAllAdapters(): MessageAdapter[] {
    return Array.from(this.adapters.values());
  }

  /**
   * Register a callback for incoming messages from any adapter
   */
  onIncomingMessage(callback: IncomingMessageCallback): void {
    this.messageCallback = callback;
  }

  /**
   * Get the adapter to use for a specific user
   * Returns the adapter they last used, or the default adapter
   */
  async getAdapterForUser(userId: string): Promise<MessageAdapter | null> {
    // Look up the user's most recent conversation to find their adapter
    const recentConversation = await db
      .select({ adapterType: conversations.adapterType })
      .from(conversations)
      .where(eq(conversations.userId, userId))
      .orderBy(desc(conversations.lastMessageAt))
      .limit(1);

    if (recentConversation.length > 0) {
      const adapter = this.adapters.get(recentConversation[0].adapterType);
      if (adapter) {
        return adapter;
      }
    }

    // Fall back to default adapter
    if (this.config.defaultAdapter) {
      return this.adapters.get(this.config.defaultAdapter) || null;
    }

    // If no default, return the first registered adapter
    const adapters = this.getAllAdapters();
    return adapters.length > 0 ? adapters[0] : null;
  }

  /**
   * Send a message to a user via their preferred adapter
   */
  async sendMessage(userId: string, content: string): Promise<void> {
    const adapter = await this.getAdapterForUser(userId);
    if (!adapter) {
      throw new Error(`No adapter found for user ${userId}`);
    }
    await adapter.sendMessage(userId, content);
  }

  /**
   * Send a typing indicator to a user via their preferred adapter
   */
  async sendTypingIndicator(userId: string): Promise<void> {
    const adapter = await this.getAdapterForUser(userId);
    if (!adapter) {
      return; // Silently ignore if no adapter
    }

    // Only send if the adapter supports typing indicators
    if (adapter.getCapabilities().typingIndicator) {
      await adapter.sendTypingIndicator(userId);
    }
  }

  /**
   * Track which adapter a user is communicating through
   * Creates or updates their conversation record
   */
  private async trackUserAdapter(
    userId: string,
    adapterName: string
  ): Promise<void> {
    // Check if user has an existing conversation with this adapter
    const existingConversation = await db
      .select()
      .from(conversations)
      .where(eq(conversations.userId, userId))
      .orderBy(desc(conversations.lastMessageAt))
      .limit(1);

    if (existingConversation.length > 0) {
      // Update the last message time
      await db
        .update(conversations)
        .set({ lastMessageAt: new Date() })
        .where(eq(conversations.id, existingConversation[0].id));
    }
    // Note: New conversations should be created by the conversation engine
    // when it processes the first message from a new user
  }
}

/**
 * Create a default message router instance
 */
export function createMessageRouter(
  config?: MessageRouterConfig
): MessageRouter {
  return new MessageRouter(config);
}
