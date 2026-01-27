/**
 * Natural language parsing utilities for Fern
 */

import * as chrono from 'chrono-node';

/**
 * Parse a natural language time string into a Date
 *
 * Examples:
 * - "tomorrow at 5pm"
 * - "in 2 hours"
 * - "next monday morning"
 * - "december 25th at noon"
 */
export function parseTime(text: string, timezone?: string): Date | null {
  const referenceDate = new Date();

  // Chrono options for parsing
  const options: chrono.ParsingOption = {
    forwardDate: true, // Prefer future dates
  };

  // Parse the text
  const results = chrono.parse(text, referenceDate, options);

  if (results.length === 0) {
    return null;
  }

  const parsed = results[0];
  const date = parsed.date();

  if (!date) {
    return null;
  }

  return date;
}

/**
 * Parse time and return both the date and a description of what was understood
 */
export function parseTimeWithDescription(text: string, timezone?: string): {
  date: Date | null;
  description: string | null;
} {
  const referenceDate = new Date();

  const results = chrono.parse(text, referenceDate, { forwardDate: true });

  if (results.length === 0) {
    return { date: null, description: null };
  }

  const parsed = results[0];
  const date = parsed.date();

  if (!date) {
    return { date: null, description: null };
  }

  // Build a description of what was parsed
  const components = parsed.start;
  const parts: string[] = [];

  if (components.get('weekday') !== undefined) {
    const days = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
    parts.push(days[components.get('weekday')!]);
  }

  if (components.get('month') !== undefined && components.get('day') !== undefined) {
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    parts.push(`${months[components.get('month')! - 1]} ${components.get('day')}`);
  }

  if (components.get('hour') !== undefined) {
    const hour = components.get('hour')!;
    const minute = components.get('minute') || 0;
    const ampm = hour >= 12 ? 'pm' : 'am';
    const hour12 = hour > 12 ? hour - 12 : (hour === 0 ? 12 : hour);
    const minuteStr = minute > 0 ? `:${minute.toString().padStart(2, '0')}` : '';
    parts.push(`${hour12}${minuteStr}${ampm}`);
  }

  return {
    date,
    description: parts.join(' ') || null,
  };
}

/**
 * Extract duration from text (e.g., "1 hour", "30 minutes", "2.5 hours")
 */
export function parseDuration(text: string): number | null {
  const lowerText = text.toLowerCase().trim();

  // Match patterns like "1 hour", "30 minutes", "1.5 hours", "90 min"
  const patterns = [
    { regex: /(\d+(?:\.\d+)?)\s*h(?:our)?s?/i, multiplier: 60 },
    { regex: /(\d+(?:\.\d+)?)\s*m(?:in(?:ute)?)?s?/i, multiplier: 1 },
  ];

  for (const { regex, multiplier } of patterns) {
    const match = lowerText.match(regex);
    if (match) {
      return parseFloat(match[1]) * multiplier;
    }
  }

  return null;
}
