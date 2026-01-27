/**
 * Configuration for Fern
 *
 * Loads configuration from environment variables and validates required settings.
 */

import { z } from 'zod';

// Configuration schema
const configSchema = z.object({
  // Server settings
  port: z.coerce.number().default(3000),
  nodeEnv: z.enum(['development', 'production', 'test']).default('development'),

  // Anthropic API
  anthropicApiKey: z.string().min(1, 'ANTHROPIC_API_KEY is required'),
  anthropicModel: z.string().default('claude-sonnet-4-20250514'),

  // Twilio settings (optional - can run without messaging)
  twilioAccountSid: z.string().optional(),
  twilioAuthToken: z.string().optional(),
  twilioPhoneNumber: z.string().optional(),

  // Webhook settings
  webhookBaseUrl: z.string().optional(),

  // Database
  databaseUrl: z.string().optional(),

  // Authentication
  authCodes: z.array(z.string()).default([]),

  // Logging
  logLevel: z.enum(['debug', 'info', 'warn', 'error']).default('info'),
});

export type Config = z.infer<typeof configSchema>;

/**
 * Load and validate configuration from environment variables
 */
function loadConfig(): Config {
  const rawConfig = {
    port: process.env.PORT,
    nodeEnv: process.env.NODE_ENV,
    anthropicApiKey: process.env.ANTHROPIC_API_KEY,
    anthropicModel: process.env.ANTHROPIC_MODEL,
    twilioAccountSid: process.env.TWILIO_ACCOUNT_SID,
    twilioAuthToken: process.env.TWILIO_AUTH_TOKEN,
    twilioPhoneNumber: process.env.TWILIO_PHONE_NUMBER,
    webhookBaseUrl: process.env.WEBHOOK_BASE_URL,
    databaseUrl: process.env.DATABASE_URL,
    authCodes: process.env.AUTH_CODES?.split(',').filter(Boolean) || [],
    logLevel: process.env.LOG_LEVEL,
  };

  const result = configSchema.safeParse(rawConfig);

  if (!result.success) {
    const errors = result.error.errors.map(
      (e) => `  - ${e.path.join('.')}: ${e.message}`
    );
    throw new Error(
      `Configuration validation failed:\n${errors.join('\n')}\n\nMake sure all required environment variables are set.`
    );
  }

  return result.data;
}

/**
 * Check if Twilio is configured
 */
export function isTwilioConfigured(config: Config): boolean {
  return !!(
    config.twilioAccountSid &&
    config.twilioAuthToken &&
    config.twilioPhoneNumber
  );
}

/**
 * Validate that required services are configured for production
 */
export function validateProductionConfig(config: Config): void {
  const warnings: string[] = [];

  if (!isTwilioConfigured(config)) {
    warnings.push('Twilio is not configured - SMS messaging will not work');
  }

  if (!config.webhookBaseUrl) {
    warnings.push('WEBHOOK_BASE_URL not set - Twilio webhook signature validation disabled');
  }

  if (config.authCodes.length === 0) {
    warnings.push('No AUTH_CODES configured - anyone can use Fern');
  }

  if (warnings.length > 0 && config.nodeEnv === 'production') {
    console.warn('Production warnings:');
    warnings.forEach((w) => console.warn(`  - ${w}`));
  }
}

// Export singleton config instance
export const config = loadConfig();

// Validate on load
validateProductionConfig(config);
