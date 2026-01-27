/**
 * User context management for Fern
 *
 * Handles loading, saving, and creating user contexts from the database.
 * The user context is the primary way Fern remembers things about users.
 */

import { db, users, type User } from '../db/client.js';
import { eq } from 'drizzle-orm';
import { randomUUID } from 'crypto';

/**
 * User identity information
 */
export interface UserIdentity {
  userId: string;
  name?: string;
  preferredName?: string;
  timezone?: string;
}

/**
 * User preferences for communication and notifications
 */
export interface UserPreferences {
  communicationStyle: 'brief' | 'detailed' | 'casual' | 'formal';
  notificationPreferences?: {
    quietHoursStart?: string; // HH:mm format
    quietHoursEnd?: string;
  };
  topicsOfInterest?: string[];
}

/**
 * Knowledge Fern has learned about a user
 */
export interface UserKnowledge {
  facts: string[];
  relationships: Record<string, string>; // e.g., { mom: "Sarah", boss: "Mike" }
  recurringEvents?: string[];
  preferences?: Record<string, string>; // e.g., { coffeeOrder: "oat milk latte" }
}

/**
 * User's conversation history summary
 */
export interface UserHistory {
  conversationSummaries: string[];
  helpedWith: string[];
  pendingReminders: string[];
}

/**
 * Complete user context for personalization
 */
export interface UserContext {
  identity: UserIdentity;
  preferences: UserPreferences;
  knowledge: UserKnowledge;
  history: UserHistory;
  primaryContact: string;
  isAuthenticated: boolean;
  createdAt: Date;
  updatedAt: Date;
}

/**
 * Default preferences for new users
 */
const defaultPreferences: UserPreferences = {
  communicationStyle: 'casual',
};

/**
 * Default empty knowledge
 */
const defaultKnowledge: UserKnowledge = {
  facts: [],
  relationships: {},
};

/**
 * Default empty history
 */
const defaultHistory: UserHistory = {
  conversationSummaries: [],
  helpedWith: [],
  pendingReminders: [],
};

/**
 * Convert database user to UserContext
 */
function userToContext(user: User): UserContext {
  const preferences = user.preferences
    ? (JSON.parse(user.preferences) as UserPreferences)
    : defaultPreferences;

  const knowledge = user.knowledge
    ? (JSON.parse(user.knowledge) as UserKnowledge)
    : defaultKnowledge;

  return {
    identity: {
      userId: user.id,
      name: user.name ?? undefined,
      timezone: user.timezone ?? undefined,
    },
    preferences,
    knowledge,
    history: defaultHistory, // History is loaded separately from messages
    primaryContact: user.primaryContact,
    isAuthenticated: !!user.name, // User is authenticated once they have a name
    createdAt: user.createdAt,
    updatedAt: user.updatedAt,
  };
}

/**
 * Load user context from database
 * @param userId The user's ID (phone number or email)
 * @returns UserContext or null if not found
 */
export async function loadUserContext(userId: string): Promise<UserContext | null> {
  const result = await db.select().from(users).where(eq(users.id, userId)).limit(1);

  if (result.length === 0) {
    return null;
  }

  return userToContext(result[0]);
}

/**
 * Save user context to database
 * @param context The user context to save
 */
export async function saveUserContext(context: UserContext): Promise<void> {
  const now = new Date();

  await db
    .update(users)
    .set({
      name: context.identity.name ?? null,
      timezone: context.identity.timezone ?? null,
      preferences: JSON.stringify(context.preferences),
      knowledge: JSON.stringify(context.knowledge),
      updatedAt: now,
    })
    .where(eq(users.id, context.identity.userId));
}

/**
 * Create a new user context in the database
 * @param userId Unique user ID (typically phone number)
 * @param primaryContact Contact info (phone or email)
 * @returns The newly created UserContext
 */
export async function createUserContext(
  userId: string,
  primaryContact: string
): Promise<UserContext> {
  const id = userId || randomUUID();
  const now = new Date();

  await db.insert(users).values({
    id,
    primaryContact,
    preferences: JSON.stringify(defaultPreferences),
    knowledge: JSON.stringify(defaultKnowledge),
    createdAt: now,
    updatedAt: now,
  });

  return {
    identity: {
      userId: id,
    },
    preferences: defaultPreferences,
    knowledge: defaultKnowledge,
    history: defaultHistory,
    primaryContact,
    isAuthenticated: false,
    createdAt: now,
    updatedAt: now,
  };
}

/**
 * Update a user's name after authentication
 * @param userId The user's ID
 * @param name The user's name
 */
export async function setUserName(userId: string, name: string): Promise<void> {
  await db.update(users).set({ name, updatedAt: new Date() }).where(eq(users.id, userId));
}

/**
 * Extract new facts from a conversation (stub for now)
 *
 * In the future, this will use the LLM to analyze conversations
 * and extract facts about the user to remember.
 *
 * @param _messages Messages from the conversation
 * @returns Array of facts extracted (empty for now)
 */
export function extractNewFacts(_messages: Array<{ role: string; content: string }>): string[] {
  // TODO: Implement fact extraction using LLM
  // This would analyze the conversation and identify things like:
  // - User's name, preferences, relationships
  // - Important dates, recurring events
  // - Preferences (coffee order, favorite restaurant, etc.)
  return [];
}

/**
 * Add a fact to a user's knowledge
 * @param context User context to update
 * @param fact The fact to add
 */
export function addFact(context: UserContext, fact: string): void {
  if (!context.knowledge.facts.includes(fact)) {
    context.knowledge.facts.push(fact);
  }
}

/**
 * Add a relationship to a user's knowledge
 * @param context User context to update
 * @param relationship Role (e.g., "mom", "boss")
 * @param name Name of the person
 */
export function addRelationship(
  context: UserContext,
  relationship: string,
  name: string
): void {
  context.knowledge.relationships[relationship] = name;
}
