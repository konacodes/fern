/**
 * Fern - A whimsical personal assistant that lives in your messages
 *
 * Main entry point - initializes all services and starts the server.
 */

import express from 'express';
import { config, isTwilioConfigured } from './config.js';
import { createLogger } from './utils/logger.js';
import { db } from './db/client.js';
import { users } from './db/schema.js';
import { TwilioAdapter } from './messaging/twilio.js';
import { MessageRouter } from './messaging/router.js';
import { conversationEngine } from './core/engine.js';
import { scheduler } from './core/scheduler.js';
import { LLMClient } from './llm/client.js';
import { buildSystemPrompt } from './llm/prompts.js';
import { loadUserContext, type UserContext } from './core/context.js';

// Import tools to register them
import './tools/index.js';

const logger = createLogger('Fern');

// Express app
const app = express();

// Parse JSON bodies
app.use(express.json());

// Health check endpoint
app.get('/health', (_req, res) => {
  res.json({ status: 'ok', service: 'fern' });
});

/**
 * Initialize messaging adapters
 */
function initializeMessaging(): MessageRouter {
  const router = new MessageRouter();

  // Set up Twilio adapter if configured
  if (isTwilioConfigured(config)) {
    logger.info('Initializing Twilio adapter');

    const twilioAdapter = new TwilioAdapter({
      accountSid: config.twilioAccountSid!,
      authToken: config.twilioAuthToken!,
      phoneNumber: config.twilioPhoneNumber!,
      validateWebhooks: !!config.webhookBaseUrl,
      webhookBaseUrl: config.webhookBaseUrl,
    });

    router.registerAdapter(twilioAdapter);

    // Mount Twilio webhook routes
    app.use(twilioAdapter.getRouter());

    logger.info('Twilio adapter initialized');
  } else {
    logger.warn('Twilio not configured - SMS messaging disabled');
  }

  return router;
}

/**
 * Initialize the LLM client and conversation engine
 */
function initializeLLM(router: MessageRouter): void {
  logger.info('Initializing LLM client');

  const llmClient = new LLMClient({
    apiKey: config.anthropicApiKey,
    model: config.anthropicModel,
  });

  // Create an adapter for the conversation engine
  const engineLLMAdapter = {
    async chat(
      systemPrompt: string,
      messages: Array<{ role: 'user' | 'assistant'; content: string }>,
      _tools: unknown
    ) {
      // Convert simple messages to Anthropic format
      const anthropicMessages = messages.map((m) => ({
        role: m.role as 'user' | 'assistant',
        content: m.content,
      }));

      // We need a user context for tools - create a minimal one
      const response = await llmClient.chat(
        systemPrompt,
        anthropicMessages,
        { userId: 'system', name: 'system' }
      );

      return {
        text: response.messages.join('\n'),
        toolCalls: response.toolCalls.map((tc, i) => ({
          id: `tool-${i}`,
          name: tc.name,
          input: tc.input as Record<string, unknown>,
        })),
      };
    },
  };

  conversationEngine.setLLMClient(engineLLMAdapter);

  // Set up system prompt builder
  conversationEngine.setSystemPromptBuilder((context: UserContext) => {
    return buildSystemPrompt({
      name: context.identity.name,
      timezone: context.identity.timezone,
      knowledge: context.knowledge as unknown as Record<string, unknown>,
      pendingReminders: context.pendingReminders,
    });
  });

  // Connect message router to conversation engine
  router.onIncomingMessage(async (userId, content, metadata) => {
    const adapter = await router.getAdapterForUser(userId);
    if (adapter) {
      await conversationEngine.handleIncomingMessage(userId, content, adapter);
    } else {
      logger.error('No adapter available for user', { userId });
    }
  });

  logger.info('LLM client initialized');
}

/**
 * Initialize the scheduler
 */
async function initializeScheduler(router: MessageRouter): Promise<void> {
  logger.info('Initializing scheduler');

  // Set up reminder callback
  scheduler.onReminderTrigger(async (userId, message, reminderId) => {
    logger.info('Triggering reminder', { userId, reminderId });

    try {
      const adapter = await router.getAdapterForUser(userId);
      if (adapter) {
        // Load user context for personalization
        const context = await loadUserContext(userId);
        const userName = context?.identity.name || 'friend';

        // Send the reminder with a friendly prefix
        await adapter.sendMessage(userId, `hey ${userName}! just a reminder:`);
        await adapter.sendMessage(userId, message);
      } else {
        logger.error('No adapter for reminder', { userId, reminderId });
      }
    } catch (error) {
      logger.error('Error sending reminder', { userId, reminderId, error });
    }
  });

  // Load pending reminders
  await scheduler.initialize();

  logger.info('Scheduler initialized');
}

/**
 * Graceful shutdown handler
 */
function setupGracefulShutdown(): void {
  const shutdown = async (signal: string) => {
    logger.info(`Received ${signal}, shutting down gracefully`);

    // Stop accepting new requests
    scheduler.shutdown();

    // Give pending requests time to complete
    await new Promise((resolve) => setTimeout(resolve, 1000));

    logger.info('Shutdown complete');
    process.exit(0);
  };

  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
}

/**
 * Main startup function
 */
async function main(): Promise<void> {
  logger.info('Starting Fern...');
  logger.info(`Environment: ${config.nodeEnv}`);

  // Test database connection
  try {
    const result = await db.select().from(users).limit(1);
    logger.info('Database connection verified');
  } catch (error) {
    logger.error('Database connection failed', { error });
    throw error;
  }

  // Initialize services
  const router = initializeMessaging();
  initializeLLM(router);
  await initializeScheduler(router);

  // Set up graceful shutdown
  setupGracefulShutdown();

  // Start server
  app.listen(config.port, () => {
    logger.info(`Fern is listening on port ${config.port}`);
    logger.info('Ready to help!');
  });
}

// Run
main().catch((error) => {
  logger.error('Failed to start Fern', { error });
  process.exit(1);
});
