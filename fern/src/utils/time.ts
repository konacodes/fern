/**
 * Timezone handling utilities for Fern
 */

/**
 * Get the current local time for a user in their timezone
 */
export function getUserLocalTime(timezone?: string): Date {
  const now = new Date();

  if (!timezone) {
    return now;
  }

  // Convert to the user's timezone
  try {
    const formatter = new Intl.DateTimeFormat('en-US', {
      timeZone: timezone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });

    const parts = formatter.formatToParts(now);
    const values: Record<string, string> = {};

    for (const part of parts) {
      values[part.type] = part.value;
    }

    // Create a date object representing the local time
    // Note: This creates a Date in the local TZ but represents the time in user's TZ
    return new Date(
      parseInt(values.year),
      parseInt(values.month) - 1,
      parseInt(values.day),
      parseInt(values.hour),
      parseInt(values.minute),
      parseInt(values.second)
    );
  } catch {
    // Invalid timezone, return UTC
    return now;
  }
}

/**
 * Format a date for display to the user in their timezone
 */
export function formatTimeForUser(date: Date, timezone?: string): string {
  const options: Intl.DateTimeFormatOptions = {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  };

  if (timezone) {
    options.timeZone = timezone;
  }

  try {
    return date.toLocaleString('en-US', options);
  } catch {
    // Invalid timezone, use default
    return date.toLocaleString('en-US', {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      hour12: true,
    });
  }
}

/**
 * Format a relative time description (e.g., "in 2 hours", "tomorrow at 5pm")
 */
export function formatRelativeTime(date: Date, timezone?: string): string {
  const now = new Date();
  const diffMs = date.getTime() - now.getTime();
  const diffMins = Math.round(diffMs / (1000 * 60));
  const diffHours = Math.round(diffMs / (1000 * 60 * 60));
  const diffDays = Math.round(diffMs / (1000 * 60 * 60 * 24));

  // Format the time portion
  const timeStr = date.toLocaleString('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
    timeZone: timezone,
  });

  if (diffMins < 60) {
    return `in ${diffMins} minute${diffMins !== 1 ? 's' : ''}`;
  }

  if (diffHours < 24) {
    return `in ${diffHours} hour${diffHours !== 1 ? 's' : ''} (${timeStr})`;
  }

  if (diffDays === 1) {
    return `tomorrow at ${timeStr}`;
  }

  if (diffDays < 7) {
    const dayName = date.toLocaleString('en-US', {
      weekday: 'long',
      timeZone: timezone,
    });
    return `${dayName} at ${timeStr}`;
  }

  return formatTimeForUser(date, timezone);
}

/**
 * Check if a timezone string is valid
 */
export function isValidTimezone(timezone: string): boolean {
  try {
    Intl.DateTimeFormat('en-US', { timeZone: timezone });
    return true;
  } catch {
    return false;
  }
}
