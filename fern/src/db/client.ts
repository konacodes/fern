import Database from 'better-sqlite3';
import { drizzle } from 'drizzle-orm/better-sqlite3';
import * as schema from './schema.js';
import { resolve, dirname } from 'path';
import { mkdirSync } from 'fs';
import { fileURLToPath } from 'url';

// Get database path from environment or use default
const getDatabasePath = (): string => {
  if (process.env.DATABASE_URL) {
    return process.env.DATABASE_URL;
  }

  // Default to data/fern.db relative to project root
  const __dirname = dirname(fileURLToPath(import.meta.url));
  const projectRoot = resolve(__dirname, '../..');
  const dataDir = resolve(projectRoot, 'data');

  // Ensure data directory exists
  mkdirSync(dataDir, { recursive: true });

  return resolve(dataDir, 'fern.db');
};

// Create the SQLite connection
const sqlite = new Database(getDatabasePath());

// Enable WAL mode for better concurrent performance
sqlite.pragma('journal_mode = WAL');

// Create drizzle instance with schema
export const db = drizzle(sqlite, { schema });

// Export the raw sqlite connection for migrations
export { sqlite };

// Export schema for convenience
export * from './schema.js';
