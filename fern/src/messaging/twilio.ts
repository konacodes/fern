/**
 * Twilio SMS adapter for Fern
 *
 * Handles SMS/MMS messaging via Twilio's API.
 */

import { Router, Request, Response } from 'express';
import twilio from 'twilio';
import {
  BaseAdapter,
  AdapterCapabilities,
  MessageMetadata,
  MediaAttachment,
} from './types.js';

/**
 * Twilio configuration
 */
export interface TwilioConfig {
  accountSid: string;
  authToken: string;
  phoneNumber: string;
  /** Optional: validate incoming webhook signatures */
  validateWebhooks?: boolean;
  /** Base URL for webhook validation */
  webhookBaseUrl?: string;
}

/**
 * Twilio SMS/MMS adapter
 */
export class TwilioAdapter extends BaseAdapter {
  readonly name = 'twilio';
  private client: twilio.Twilio;
  private config: TwilioConfig;
  private router: Router;

  constructor(config: TwilioConfig) {
    super();
    this.config = config;
    this.client = twilio(config.accountSid, config.authToken);
    this.router = this.createRouter();
  }

  /**
   * Get the Express router for Twilio webhooks
   */
  getRouter(): Router {
    return this.router;
  }

  /**
   * Create Express router for handling Twilio webhooks
   */
  private createRouter(): Router {
    const router = Router();

    // Parse URL-encoded bodies (Twilio sends webhooks this way)
    router.use(require('express').urlencoded({ extended: false }));

    // Optionally validate Twilio signatures
    if (this.config.validateWebhooks && this.config.webhookBaseUrl) {
      router.use(this.validateSignature.bind(this));
    }

    // Main webhook endpoint
    router.post('/webhook/twilio', this.handleWebhook.bind(this));

    return router;
  }

  /**
   * Middleware to validate Twilio request signatures
   */
  private validateSignature(req: Request, res: Response, next: Function): void {
    const signature = req.headers['x-twilio-signature'] as string;
    const url = `${this.config.webhookBaseUrl}${req.originalUrl}`;

    const isValid = twilio.validateRequest(
      this.config.authToken,
      signature,
      url,
      req.body
    );

    if (!isValid) {
      res.status(403).send('Invalid signature');
      return;
    }

    next();
  }

  /**
   * Handle incoming Twilio webhook
   */
  private async handleWebhook(req: Request, res: Response): Promise<void> {
    const body = req.body;

    // Extract message data from Twilio webhook
    const userId = body.From; // Phone number in E.164 format
    const content = body.Body || '';
    const messageId = body.MessageSid;
    const numMedia = parseInt(body.NumMedia || '0', 10);

    // Extract media attachments if present
    const media: MediaAttachment[] = [];
    for (let i = 0; i < numMedia; i++) {
      const mediaUrl = body[`MediaUrl${i}`];
      const mediaType = body[`MediaContentType${i}`];
      if (mediaUrl) {
        media.push({
          type: this.getMediaType(mediaType),
          url: mediaUrl,
          mimeType: mediaType,
        });
      }
    }

    const metadata: MessageMetadata = {
      messageId,
      timestamp: new Date(),
      media: media.length > 0 ? media : undefined,
      raw: body,
    };

    // Respond immediately with empty TwiML to acknowledge receipt
    // This prevents Twilio from retrying the webhook
    const twiml = new twilio.twiml.MessagingResponse();
    res.type('text/xml');
    res.send(twiml.toString());

    // Process the message asynchronously
    try {
      await this.handleIncoming(userId, content, metadata);
    } catch (error) {
      console.error('[TwilioAdapter] Error processing message:', error);
    }
  }

  /**
   * Convert MIME type to MediaAttachment type
   */
  private getMediaType(mimeType: string): MediaAttachment['type'] {
    if (mimeType.startsWith('image/')) return 'image';
    if (mimeType.startsWith('audio/')) return 'audio';
    if (mimeType.startsWith('video/')) return 'video';
    return 'file';
  }

  /**
   * Send a message via Twilio
   */
  async sendMessage(userId: string, content: string): Promise<void> {
    // Split long messages if needed (SMS limit is 1600 chars for multipart)
    const maxLength = 1600;
    const messages = this.splitMessage(content, maxLength);

    for (const msg of messages) {
      await this.client.messages.create({
        body: msg,
        from: this.config.phoneNumber,
        to: userId,
      });
    }
  }

  /**
   * Split a message into chunks that fit within the limit
   */
  private splitMessage(content: string, maxLength: number): string[] {
    if (content.length <= maxLength) {
      return [content];
    }

    const messages: string[] = [];
    let remaining = content;

    while (remaining.length > 0) {
      if (remaining.length <= maxLength) {
        messages.push(remaining);
        break;
      }

      // Try to split at a natural break point
      let splitIndex = remaining.lastIndexOf('\n', maxLength);
      if (splitIndex === -1 || splitIndex < maxLength / 2) {
        splitIndex = remaining.lastIndexOf(' ', maxLength);
      }
      if (splitIndex === -1 || splitIndex < maxLength / 2) {
        splitIndex = maxLength;
      }

      messages.push(remaining.slice(0, splitIndex).trim());
      remaining = remaining.slice(splitIndex).trim();
    }

    return messages;
  }

  /**
   * Twilio doesn't support typing indicators for SMS
   * This is a no-op but included for interface compliance
   */
  async sendTypingIndicator(_userId: string): Promise<void> {
    // SMS doesn't support typing indicators
    // Could potentially be used for RCS in the future
  }

  /**
   * Get adapter capabilities
   */
  getCapabilities(): AdapterCapabilities {
    return {
      typingIndicator: false, // SMS doesn't support this
      readReceipts: false, // SMS doesn't have read receipts
      reactions: false, // SMS doesn't support reactions
      media: true, // MMS supports media
      richCards: false, // Standard SMS doesn't support rich cards
      maxMessageLength: 1600, // Multipart SMS limit
    };
  }
}

/**
 * Create a Twilio adapter from environment variables
 */
export function createTwilioAdapterFromEnv(): TwilioAdapter {
  const accountSid = process.env.TWILIO_ACCOUNT_SID;
  const authToken = process.env.TWILIO_AUTH_TOKEN;
  const phoneNumber = process.env.TWILIO_PHONE_NUMBER;
  const webhookBaseUrl = process.env.WEBHOOK_BASE_URL;

  if (!accountSid || !authToken || !phoneNumber) {
    throw new Error(
      'Missing Twilio configuration. Set TWILIO_ACCOUNT_SID, TWILIO_AUTH_TOKEN, and TWILIO_PHONE_NUMBER environment variables.'
    );
  }

  return new TwilioAdapter({
    accountSid,
    authToken,
    phoneNumber,
    validateWebhooks: !!webhookBaseUrl,
    webhookBaseUrl,
  });
}
