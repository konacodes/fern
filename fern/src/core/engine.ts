/**
 * Conversation Engine for Fern
 *
 * The brain that orchestrates everything - handles incoming messages,
 * coordinates with the LLM, executes tools, and sends responses.
 */

import { db, messages, conversations, type Message } from '../db/client.js';
import { eq, desc, and } from 'drizzle-orm';
import { randomUUID } from 'crypto';
import type { MessageAdapter } from '../messaging/types.js';
import {
  loadUserContext,
  createUserContext,
  saveUserContext,
  setUserName,
  type UserContext,
} from './context.js';
import { executeTool, getToolsForAnthropic, type UserContext as ToolUserContext } from '../llm/tools.js';

/**
 * Authentication codes for new users
 * Loaded from AUTH_CODES environment variable (comma-separated)
 */
const AUTH_CODES = new Set((process.env.AUTH_CODES || '').split(',').filter(Boolean));

/**
 * Chain sender for natural message rhythm
 *
 * Sends messages like a real person texting - short bursts with small delays.
 */
export class ChainSender {
  private userId: string;
  private adapter: MessageAdapter;
  private messageQueue: string[] = [];
  private isSending = false;

  constructor(userId: string, adapter: MessageAdapter) {
    this.userId = userId;
    this.adapter = adapter;
  }

  /**
   * Queue a message to send
   * Messages are sent with natural delays between them
   */
  async send(message: string): Promise<void> {
    this.messageQueue.push(message);
    if (!this.isSending) {
      await this.processQueue();
    }
  }

  /**
   * Process queued messages with natural timing
   */
  private async processQueue(): Promise<void> {
    this.isSending = true;

    while (this.messageQueue.length > 0) {
      const message = this.messageQueue.shift()!;

      // Send typing indicator first
      await this.adapter.sendTypingIndicator(this.userId);

      // Small delay to simulate typing
      await this.delay(100 + Math.random() * 200);

      // Send the message
      await this.adapter.sendMessage(this.userId, message);

      // Natural pause between messages (300-500ms)
      if (this.messageQueue.length > 0) {
        await this.delay(300 + Math.random() * 200);
      }
    }

    this.isSending = false;
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

/**
 * Conversation lock to prevent race conditions
 * Maps userId to a promise that resolves when the conversation is unlocked
 */
const conversationLocks = new Map<string, Promise<void>>();

/**
 * Acquire a lock for a user's conversation
 */
async function acquireLock(userId: string): Promise<() => void> {
  // Wait for any existing lock
  while (conversationLocks.has(userId)) {
    await conversationLocks.get(userId);
  }

  // Create a new lock
  let releaseLock: () => void;
  const lockPromise = new Promise<void>((resolve) => {
    releaseLock = resolve;
  });

  conversationLocks.set(userId, lockPromise);

  return () => {
    conversationLocks.delete(userId);
    releaseLock!();
  };
}

/**
 * Message format for LLM
 */
interface LLMMessage {
  role: 'user' | 'assistant';
  content: string;
}

/**
 * Get or create an active conversation for a user
 */
async function getOrCreateConversation(
  userId: string,
  adapterType: string
): Promise<string> {
  // Look for existing recent conversation (within last 30 minutes)
  const thirtyMinutesAgo = new Date(Date.now() - 30 * 60 * 1000);

  const existing = await db
    .select()
    .from(conversations)
    .where(
      and(eq(conversations.userId, userId), eq(conversations.adapterType, adapterType))
    )
    .orderBy(desc(conversations.lastMessageAt))
    .limit(1);

  if (existing.length > 0 && existing[0].lastMessageAt > thirtyMinutesAgo) {
    // Update last message time
    await db
      .update(conversations)
      .set({ lastMessageAt: new Date() })
      .where(eq(conversations.id, existing[0].id));
    return existing[0].id;
  }

  // Create new conversation
  const id = randomUUID();
  await db.insert(conversations).values({
    id,
    userId,
    adapterType,
    startedAt: new Date(),
    lastMessageAt: new Date(),
  });

  return id;
}

/**
 * Get recent message history for a user
 */
async function getRecentHistory(
  conversationId: string,
  limit = 30
): Promise<LLMMessage[]> {
  const recentMessages = await db
    .select()
    .from(messages)
    .where(eq(messages.conversationId, conversationId))
    .orderBy(desc(messages.createdAt))
    .limit(limit);

  // Reverse to get chronological order
  return recentMessages.reverse().map((m) => ({
    role: m.role,
    content: m.content,
  }));
}

/**
 * Save a message to the database
 */
async function saveMessage(
  conversationId: string,
  role: 'user' | 'assistant',
  content: string,
  metadata?: Record<string, unknown>
): Promise<void> {
  await db.insert(messages).values({
    id: randomUUID(),
    conversationId,
    role,
    content,
    metadata: metadata ? JSON.stringify(metadata) : null,
    createdAt: new Date(),
  });
}

/**
 * Convert UserContext to ToolUserContext for tool execution
 */
function toToolUserContext(context: UserContext): ToolUserContext {
  return {
    userId: context.identity.userId,
    name: context.identity.name,
    timezone: context.identity.timezone,
    preferences: context.preferences as unknown as Record<string, unknown>,
    knowledge: context.knowledge as unknown as Record<string, unknown>,
  };
}

/**
 * Main Conversation Engine class
 */
export class ConversationEngine {
  private llmClient: LLMClientInterface | null = null;
  private systemPromptBuilder: ((context: UserContext) => string) | null = null;

  /**
   * Set the LLM client to use for generating responses
   */
  setLLMClient(client: LLMClientInterface): void {
    this.llmClient = client;
  }

  /**
   * Set the system prompt builder function
   */
  setSystemPromptBuilder(builder: (context: UserContext) => string): void {
    this.systemPromptBuilder = builder;
  }

  /**
   * Handle an incoming message from any adapter
   *
   * This is the main entry point for all messages.
   */
  async handleIncomingMessage(
    userId: string,
    content: string,
    adapter: MessageAdapter
  ): Promise<void> {
    // Acquire conversation lock to prevent race conditions
    const releaseLock = await acquireLock(userId);

    try {
      // Load or create user context
      let context = await loadUserContext(userId);
      let isNewUser = false;

      if (!context) {
        context = await createUserContext(userId, userId);
        isNewUser = true;
      }

      // Get or create conversation
      const conversationId = await getOrCreateConversation(userId, adapter.name);

      // Save incoming message
      await saveMessage(conversationId, 'user', content);

      // Create chain sender for natural message rhythm
      const chain = new ChainSender(userId, adapter);

      // Handle new user authentication flow
      if (isNewUser || !context.isAuthenticated) {
        await this.handleAuthFlow(context, content, chain, conversationId);
        return;
      }

      // Generate and send response
      await this.generateResponse(context, conversationId, chain);
    } finally {
      releaseLock();
    }
  }

  /**
   * Handle authentication flow for new users
   */
  private async handleAuthFlow(
    context: UserContext,
    content: string,
    chain: ChainSender,
    conversationId: string
  ): Promise<void> {
    const trimmedContent = content.trim().toLowerCase();

    // Check if this is the first message (ask for magic word)
    if (!context.identity.name && AUTH_CODES.size > 0) {
      // Check if content is a valid auth code
      if (AUTH_CODES.has(trimmedContent) || AUTH_CODES.has(content.trim())) {
        // Valid code - ask for name
        await chain.send("Welcome! I'm Fern. What's your name? 🌿");
        await saveMessage(
          conversationId,
          'assistant',
          "Welcome! I'm Fern. What's your name? 🌿"
        );
        // Mark as partially authenticated (they know the code)
        context.knowledge.facts.push('has_valid_auth_code');
        await saveUserContext(context);
      } else if (context.knowledge.facts.includes('has_valid_auth_code')) {
        // They've already provided the code, this is their name
        const name = content.trim();
        await setUserName(context.identity.userId, name);
        context.identity.name = name;
        context.isAuthenticated = true;
        await saveUserContext(context);

        await chain.send(`nice to meet you, ${name}!`);
        await chain.send("i'm here to help with whatever you need");
        await chain.send('just text me anytime 💚');

        await saveMessage(
          conversationId,
          'assistant',
          `nice to meet you, ${name}! i'm here to help with whatever you need. just text me anytime 💚`
        );
      } else {
        // Unknown user, ask for magic word
        await chain.send("Hey! I don't think we've met. What's the magic word? 🌿");
        await saveMessage(
          conversationId,
          'assistant',
          "Hey! I don't think we've met. What's the magic word? 🌿"
        );
      }
    } else if (AUTH_CODES.size === 0) {
      // No auth codes configured - auto-authenticate
      // This is their name
      const name = content.trim();
      await setUserName(context.identity.userId, name);
      context.identity.name = name;
      context.isAuthenticated = true;
      await saveUserContext(context);

      await chain.send(`hey ${name}! i'm fern 🌿`);
      await chain.send("i'm here to help with whatever you need");

      await saveMessage(
        conversationId,
        'assistant',
        `hey ${name}! i'm fern 🌿 i'm here to help with whatever you need`
      );
    }
  }

  /**
   * Generate a response using the LLM
   */
  private async generateResponse(
    context: UserContext,
    conversationId: string,
    chain: ChainSender
  ): Promise<void> {
    if (!this.llmClient || !this.systemPromptBuilder) {
      // Fallback response if LLM not configured
      await chain.send("i'm having a bit of trouble thinking right now");
      await chain.send('try again in a moment?');
      return;
    }

    // Get conversation history
    const history = await getRecentHistory(conversationId);

    // Build system prompt
    const systemPrompt = this.systemPromptBuilder(context);

    // Get available tools
    const tools = getToolsForAnthropic();

    try {
      // Call LLM
      const response = await this.llmClient.chat(systemPrompt, history, tools);

      // Process response
      await this.processLLMResponse(
        response,
        context,
        conversationId,
        chain,
        systemPrompt,
        history
      );
    } catch (error) {
      console.error('LLM error:', error);
      await chain.send('hmm, something went wrong on my end');
      await chain.send("let's try that again?");
    }
  }

  /**
   * Process LLM response, handling tool calls if needed
   */
  private async processLLMResponse(
    response: LLMResponse,
    context: UserContext,
    conversationId: string,
    chain: ChainSender,
    systemPrompt: string,
    history: LLMMessage[]
  ): Promise<void> {
    // Handle tool calls
    if (response.toolCalls && response.toolCalls.length > 0) {
      for (const toolCall of response.toolCalls) {
        // Send acknowledgment if there's one
        if (response.text) {
          await chain.send(response.text);
        }

        // Execute the tool
        const toolUserContext = toToolUserContext(context);
        const result = await executeTool(toolCall.name, toolCall.input, toolUserContext);

        // Continue with tool result
        if (this.llmClient) {
          const toolResultMessage: LLMMessage = {
            role: 'assistant',
            content: `Tool ${toolCall.name} result: ${JSON.stringify(result)}`,
          };

          const continuedResponse = await this.llmClient.chat(
            systemPrompt,
            [...history, toolResultMessage],
            getToolsForAnthropic()
          );

          // Recursively process the continued response
          await this.processLLMResponse(
            continuedResponse,
            context,
            conversationId,
            chain,
            systemPrompt,
            [...history, toolResultMessage]
          );
          return;
        }
      }
    }

    // Send text response
    if (response.text) {
      // Split long responses into multiple messages
      const messages = splitIntoMessages(response.text);
      for (const msg of messages) {
        await chain.send(msg);
      }

      // Save the complete response
      await saveMessage(conversationId, 'assistant', response.text);
    }
  }
}

/**
 * Split text into natural message chunks
 * Max ~2 sentences per message for natural texting rhythm
 */
function splitIntoMessages(text: string): string[] {
  // If text is short enough, return as-is
  if (text.length < 100) {
    return [text];
  }

  const messages: string[] = [];
  const sentences = text.split(/(?<=[.!?])\s+/);
  let current = '';

  for (const sentence of sentences) {
    if (current.length + sentence.length > 150 || current.split(/[.!?]/).length > 2) {
      if (current.trim()) {
        messages.push(current.trim());
      }
      current = sentence;
    } else {
      current += (current ? ' ' : '') + sentence;
    }
  }

  if (current.trim()) {
    messages.push(current.trim());
  }

  return messages.length > 0 ? messages : [text];
}

/**
 * LLM Client interface for dependency injection
 */
export interface LLMClientInterface {
  chat(
    systemPrompt: string,
    messages: LLMMessage[],
    tools: ReturnType<typeof getToolsForAnthropic>
  ): Promise<LLMResponse>;
}

/**
 * LLM Response structure
 */
export interface LLMResponse {
  text?: string;
  toolCalls?: Array<{
    id: string;
    name: string;
    input: Record<string, unknown>;
  }>;
}

// Export singleton instance
export const conversationEngine = new ConversationEngine();
