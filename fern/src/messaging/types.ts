/**
 * Messaging adapter interface and types for Fern
 *
 * All messaging platforms implement a common interface so Fern
 * doesn't care if you're on iMessage, SMS, WhatsApp, or anything else.
 */

/**
 * Capabilities that an adapter may support
 */
export interface AdapterCapabilities {
  /** Can show typing indicators */
  typingIndicator: boolean;
  /** Can send read receipts */
  readReceipts: boolean;
  /** Can send reactions to messages */
  reactions: boolean;
  /** Can send media (images, files) */
  media: boolean;
  /** Can send rich cards/templates */
  richCards: boolean;
  /** Maximum message length (0 = no limit) */
  maxMessageLength: number;
}

/**
 * Callback function type for incoming messages
 */
export type IncomingMessageCallback = (
  userId: string,
  content: string,
  metadata?: MessageMetadata
) => Promise<void>;

/**
 * Optional metadata attached to messages
 */
export interface MessageMetadata {
  /** Original message ID from the platform */
  messageId?: string;
  /** Timestamp of the message */
  timestamp?: Date;
  /** Media attachments */
  media?: MediaAttachment[];
  /** Raw platform-specific data */
  raw?: unknown;
}

/**
 * Media attachment (images, files, etc.)
 */
export interface MediaAttachment {
  type: 'image' | 'audio' | 'video' | 'file';
  url: string;
  mimeType?: string;
  filename?: string;
}

/**
 * Core messaging adapter interface
 *
 * All adapters must implement this interface to work with Fern.
 */
export interface MessageAdapter {
  /** Unique name identifying this adapter (e.g., 'twilio', 'bluebubbles') */
  readonly name: string;

  /**
   * Register a callback to handle incoming messages
   * @param callback Function called when a message arrives
   */
  onIncomingMessage(callback: IncomingMessageCallback): void;

  /**
   * Send a text message to a user
   * @param userId User identifier (phone number, iCloud email, etc.)
   * @param content Message text to send
   */
  sendMessage(userId: string, content: string): Promise<void>;

  /**
   * Show typing indicator to user (if supported)
   * @param userId User identifier
   */
  sendTypingIndicator(userId: string): Promise<void>;

  /**
   * Get the capabilities of this adapter
   */
  getCapabilities(): AdapterCapabilities;
}

/**
 * Base class for adapters with common functionality
 */
export abstract class BaseAdapter implements MessageAdapter {
  abstract readonly name: string;

  protected messageCallback: IncomingMessageCallback | null = null;

  onIncomingMessage(callback: IncomingMessageCallback): void {
    this.messageCallback = callback;
  }

  abstract sendMessage(userId: string, content: string): Promise<void>;
  abstract sendTypingIndicator(userId: string): Promise<void>;
  abstract getCapabilities(): AdapterCapabilities;

  /**
   * Helper to invoke the message callback safely
   */
  protected async handleIncoming(
    userId: string,
    content: string,
    metadata?: MessageMetadata
  ): Promise<void> {
    if (this.messageCallback) {
      await this.messageCallback(userId, content, metadata);
    }
  }
}
