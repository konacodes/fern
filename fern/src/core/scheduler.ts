/**
 * Reminder and Job Scheduler for Fern
 *
 * Handles scheduling and triggering reminders. Fern needs to reach out
 * proactively, not just respond to messages.
 */

import cron from 'node-cron';
import { db, reminders, type Reminder } from '../db/client.js';
import { eq, and, lte } from 'drizzle-orm';
import { randomUUID } from 'crypto';

/**
 * Callback type for when a reminder triggers
 */
export type ReminderCallback = (
  userId: string,
  message: string,
  reminderId: string
) => Promise<void>;

/**
 * Scheduled job reference
 */
interface ScheduledJob {
  reminderId: string;
  task: cron.ScheduledTask;
}

/**
 * Reminder Scheduler class
 *
 * Manages all scheduled reminders using node-cron.
 */
export class Scheduler {
  private jobs: Map<string, ScheduledJob> = new Map();
  private reminderCallback: ReminderCallback | null = null;
  private checkTask: cron.ScheduledTask | null = null;

  /**
   * Set the callback function for when reminders trigger
   */
  onReminderTrigger(callback: ReminderCallback): void {
    this.reminderCallback = callback;
  }

  /**
   * Initialize the scheduler - load pending reminders from database
   */
  async initialize(): Promise<void> {
    console.log('[Scheduler] Initializing...');

    // Load all pending reminders
    const pendingReminders = await db
      .select()
      .from(reminders)
      .where(
        and(
          eq(reminders.status, 'pending'),
          eq(reminders.triggerType, 'time')
        )
      );

    console.log(`[Scheduler] Found ${pendingReminders.length} pending reminders`);

    // Schedule each reminder
    for (const reminder of pendingReminders) {
      this.scheduleReminderJob(reminder);
    }

    // Start periodic check for due reminders (every minute)
    // This catches any reminders that might have been missed
    this.checkTask = cron.schedule('* * * * *', () => {
      this.checkDueReminders().catch(console.error);
    });

    console.log('[Scheduler] Initialization complete');
  }

  /**
   * Schedule a new reminder
   *
   * @param userId User to remind
   * @param message Reminder message
   * @param triggerTime When to trigger the reminder
   * @returns The created reminder ID
   */
  async scheduleReminder(
    userId: string,
    message: string,
    triggerTime: Date
  ): Promise<string> {
    const id = randomUUID();

    // Store in database
    await db.insert(reminders).values({
      id,
      userId,
      message,
      triggerType: 'time',
      triggerValue: triggerTime.toISOString(),
      status: 'pending',
      createdAt: new Date(),
    });

    // Get the inserted reminder
    const [reminder] = await db
      .select()
      .from(reminders)
      .where(eq(reminders.id, id))
      .limit(1);

    // Schedule the job
    this.scheduleReminderJob(reminder);

    console.log(`[Scheduler] Scheduled reminder ${id} for ${triggerTime.toISOString()}`);

    return id;
  }

  /**
   * Schedule a conditional reminder (e.g., "when I get home")
   * These are stored but not scheduled until the condition is met
   *
   * @param userId User to remind
   * @param message Reminder message
   * @param condition The condition that triggers the reminder
   * @returns The created reminder ID
   */
  async scheduleConditionalReminder(
    userId: string,
    message: string,
    condition: string
  ): Promise<string> {
    const id = randomUUID();

    // Store in database (not scheduled as a cron job)
    await db.insert(reminders).values({
      id,
      userId,
      message,
      triggerType: 'condition',
      triggerValue: condition,
      status: 'pending',
      createdAt: new Date(),
    });

    console.log(`[Scheduler] Created conditional reminder ${id}: "${condition}"`);

    return id;
  }

  /**
   * Cancel a scheduled reminder
   */
  async cancelReminder(reminderId: string): Promise<boolean> {
    // Update status in database
    const result = await db
      .update(reminders)
      .set({ status: 'cancelled' })
      .where(eq(reminders.id, reminderId));

    // Cancel the scheduled job if it exists
    const job = this.jobs.get(reminderId);
    if (job) {
      job.task.stop();
      this.jobs.delete(reminderId);
    }

    console.log(`[Scheduler] Cancelled reminder ${reminderId}`);

    return true;
  }

  /**
   * Get all pending reminders for a user
   */
  async getPendingReminders(userId: string): Promise<Reminder[]> {
    return db
      .select()
      .from(reminders)
      .where(
        and(eq(reminders.userId, userId), eq(reminders.status, 'pending'))
      );
  }

  /**
   * Schedule a cron job for a reminder
   */
  private scheduleReminderJob(reminder: Reminder): void {
    const triggerTime = new Date(reminder.triggerValue);
    const now = new Date();

    // If the trigger time has already passed, trigger immediately
    if (triggerTime <= now) {
      console.log(`[Scheduler] Reminder ${reminder.id} is overdue, triggering now`);
      this.triggerReminder(reminder).catch(console.error);
      return;
    }

    // Calculate delay until trigger time
    const delay = triggerTime.getTime() - now.getTime();

    // For reminders more than 24 hours away, we'll rely on the periodic check
    if (delay > 24 * 60 * 60 * 1000) {
      console.log(`[Scheduler] Reminder ${reminder.id} is >24h away, using periodic check`);
      return;
    }

    // Schedule using setTimeout for precision
    const timeoutId = setTimeout(() => {
      this.triggerReminder(reminder).catch(console.error);
    }, delay);

    // Wrap in a ScheduledTask-like object
    const task = {
      stop: () => clearTimeout(timeoutId),
    } as cron.ScheduledTask;

    this.jobs.set(reminder.id, { reminderId: reminder.id, task });

    console.log(
      `[Scheduler] Scheduled reminder ${reminder.id} for ${triggerTime.toISOString()} (in ${Math.round(delay / 1000 / 60)} minutes)`
    );
  }

  /**
   * Trigger a reminder
   */
  private async triggerReminder(reminder: Reminder): Promise<void> {
    console.log(`[Scheduler] Triggering reminder ${reminder.id}`);

    // Mark as delivered
    await db
      .update(reminders)
      .set({
        status: 'delivered',
        triggeredAt: new Date(),
      })
      .where(eq(reminders.id, reminder.id));

    // Remove from active jobs
    this.jobs.delete(reminder.id);

    // Call the callback if set
    if (this.reminderCallback) {
      try {
        await this.reminderCallback(reminder.userId, reminder.message, reminder.id);
      } catch (error) {
        console.error(`[Scheduler] Error triggering reminder ${reminder.id}:`, error);
      }
    }
  }

  /**
   * Check for due reminders (called periodically)
   */
  private async checkDueReminders(): Promise<void> {
    const now = new Date();

    // Find all time-based reminders that are due
    const dueReminders = await db
      .select()
      .from(reminders)
      .where(
        and(
          eq(reminders.status, 'pending'),
          eq(reminders.triggerType, 'time')
        )
      );

    for (const reminder of dueReminders) {
      const triggerTime = new Date(reminder.triggerValue);

      if (triggerTime <= now && !this.jobs.has(reminder.id)) {
        // This reminder is due and not already being processed
        await this.triggerReminder(reminder);
      }
    }
  }

  /**
   * Check if a condition matches any conditional reminders
   * Called when relevant signals arrive (e.g., user says "I'm home")
   *
   * @param userId User whose reminders to check
   * @param condition The condition that occurred
   */
  async checkCondition(userId: string, condition: string): Promise<Reminder[]> {
    // Find conditional reminders for this user
    const conditionalReminders = await db
      .select()
      .from(reminders)
      .where(
        and(
          eq(reminders.userId, userId),
          eq(reminders.status, 'pending'),
          eq(reminders.triggerType, 'condition')
        )
      );

    const matchedReminders: Reminder[] = [];

    // Simple string matching for now
    // TODO: Use LLM for smarter condition matching
    const normalizedCondition = condition.toLowerCase();

    for (const reminder of conditionalReminders) {
      const reminderCondition = reminder.triggerValue.toLowerCase();

      // Check for common condition matches
      if (
        (reminderCondition.includes('home') && normalizedCondition.includes('home')) ||
        (reminderCondition.includes('work') && normalizedCondition.includes('work')) ||
        normalizedCondition.includes(reminderCondition)
      ) {
        matchedReminders.push(reminder);
        await this.triggerReminder(reminder);
      }
    }

    return matchedReminders;
  }

  /**
   * Stop all scheduled jobs and clean up
   */
  shutdown(): void {
    console.log('[Scheduler] Shutting down...');

    // Stop the periodic check
    if (this.checkTask) {
      this.checkTask.stop();
      this.checkTask = null;
    }

    // Stop all scheduled reminder jobs
    for (const [id, job] of this.jobs) {
      job.task.stop();
      console.log(`[Scheduler] Stopped job ${id}`);
    }
    this.jobs.clear();

    console.log('[Scheduler] Shutdown complete');
  }
}

// Export singleton instance
export const scheduler = new Scheduler();
