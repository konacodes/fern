import { z } from 'zod';
import { Tool, registerTool, UserContext, ToolResult } from '../llm/tools.js';

// Parameters schema for SendEmail tool
const SendEmailParams = z.object({
  to: z.string().describe('Email recipient address'),
  subject: z.string().describe('Email subject line'),
  body: z.string().describe('Email body content'),
  draft: z.boolean().optional().default(true).describe('If true (default), save as draft instead of sending immediately'),
});

/**
 * SendEmail Tool - Draft or send an email on behalf of the user
 *
 * NOTE: This is a stub implementation that logs the email details.
 * Future implementation will integrate with Gmail, Apple Mail, etc.
 *
 * For safety, draft mode is enabled by default - emails are saved as drafts
 * rather than sent immediately unless explicitly requested.
 */
export const SendEmailTool: Tool<typeof SendEmailParams> = {
  name: 'SendEmail',
  description: 'Draft or send an email on behalf of the user. By default, emails are saved as drafts for review. Set draft to false to send immediately (use with caution).',
  parameters: SendEmailParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { to, subject, body, draft = true } = params;

    console.log(`[Email] ${draft ? 'Drafting' : 'Sending'} email for ${userContext.userId}:`, {
      to,
      subject,
      bodyPreview: body.substring(0, 100) + (body.length > 100 ? '...' : ''),
      draft,
    });

    // Generate a mock email/draft ID
    const emailId = `mock-${draft ? 'draft' : 'sent'}-${Date.now()}`;

    if (draft) {
      return {
        success: true,
        data: {
          emailId,
          status: 'drafted',
          to,
          subject,
          note: 'Email saved as draft (stub). Email integration coming soon!',
        },
      };
    }

    return {
      success: true,
      data: {
        emailId,
        status: 'sent',
        to,
        subject,
        note: 'Email sent (stub). Email integration coming soon!',
      },
    };
  },
};

// Register the tool
registerTool(SendEmailTool);

export default SendEmailTool;
