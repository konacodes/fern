/**
 * Simple logging utility for Fern
 */

export enum LogLevel {
  DEBUG = 0,
  INFO = 1,
  WARN = 2,
  ERROR = 3,
}

// Default log level from environment or INFO
const LOG_LEVEL_MAP: Record<string, LogLevel> = {
  debug: LogLevel.DEBUG,
  info: LogLevel.INFO,
  warn: LogLevel.WARN,
  error: LogLevel.ERROR,
};

let currentLevel = LOG_LEVEL_MAP[process.env.LOG_LEVEL?.toLowerCase() || 'info'] ?? LogLevel.INFO;

/**
 * Set the current log level
 */
export function setLogLevel(level: LogLevel): void {
  currentLevel = level;
}

/**
 * Get current timestamp for log entries
 */
function timestamp(): string {
  return new Date().toISOString();
}

/**
 * Format a log message with optional context
 */
function formatMessage(level: string, context: string, message: string, data?: unknown): string {
  const base = `[${timestamp()}] [${level}] [${context}] ${message}`;
  if (data !== undefined) {
    return `${base} ${JSON.stringify(data)}`;
  }
  return base;
}

/**
 * Create a logger instance for a specific context (module/component)
 */
export function createLogger(context: string) {
  return {
    debug(message: string, data?: unknown): void {
      if (currentLevel <= LogLevel.DEBUG) {
        console.debug(formatMessage('DEBUG', context, message, data));
      }
    },

    info(message: string, data?: unknown): void {
      if (currentLevel <= LogLevel.INFO) {
        console.info(formatMessage('INFO', context, message, data));
      }
    },

    warn(message: string, data?: unknown): void {
      if (currentLevel <= LogLevel.WARN) {
        console.warn(formatMessage('WARN', context, message, data));
      }
    },

    error(message: string, data?: unknown): void {
      if (currentLevel <= LogLevel.ERROR) {
        console.error(formatMessage('ERROR', context, message, data));
      }
    },
  };
}

// Default logger for general use
export const logger = createLogger('Fern');
