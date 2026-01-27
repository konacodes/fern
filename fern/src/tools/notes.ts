import { z } from 'zod';
import { like } from 'drizzle-orm';
import { Tool, registerTool, UserContext, ToolResult } from '../llm/tools.js';
import { db, notes } from '../db/client.js';

// Parameters schema for TakeNote tool
const TakeNoteParams = z.object({
  content: z.string().describe('The note content to save'),
  tags: z.array(z.string()).optional().describe('Optional tags to categorize the note'),
});

// Parameters schema for RecallNote tool
const RecallNoteParams = z.object({
  query: z.string().describe('Search query to find notes'),
});

/**
 * TakeNote Tool - Save a note for the user
 */
export const TakeNoteTool: Tool<typeof TakeNoteParams> = {
  name: 'TakeNote',
  description: 'Save a note for the user. Notes are stored persistently and can be recalled later.',
  parameters: TakeNoteParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { content, tags } = params;

    // Generate a unique ID for the note
    const id = crypto.randomUUID();

    // Store the note in the database
    await db.insert(notes).values({
      id,
      userId: userContext.userId,
      content,
      tags: tags ? JSON.stringify(tags) : undefined,
    });

    return {
      success: true,
      data: {
        noteId: id,
        content: content.substring(0, 100) + (content.length > 100 ? '...' : ''),
        tags: tags || [],
      },
    };
  },
};

/**
 * RecallNote Tool - Search through user's saved notes
 */
export const RecallNoteTool: Tool<typeof RecallNoteParams> = {
  name: 'RecallNote',
  description: 'Search through the user\'s saved notes. Returns notes that match the search query.',
  parameters: RecallNoteParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { query } = params;

    // Search for notes matching the query
    // Using LIKE for simple text search (future: semantic search with embeddings)
    const matchingNotes = await db
      .select()
      .from(notes)
      .where(like(notes.content, `%${query}%`));

    // Filter by user (in case we want to add this later)
    const userNotes = matchingNotes.filter(n => n.userId === userContext.userId);

    if (userNotes.length === 0) {
      return {
        success: true,
        data: {
          notes: [],
          message: `No notes found matching "${query}"`,
        },
      };
    }

    // Format notes for response
    const formattedNotes = userNotes.map(note => ({
      id: note.id,
      content: note.content,
      tags: note.tags ? JSON.parse(note.tags) : [],
      createdAt: note.createdAt,
    }));

    return {
      success: true,
      data: {
        notes: formattedNotes,
        count: formattedNotes.length,
      },
    };
  },
};

// Register the tools
registerTool(TakeNoteTool);
registerTool(RecallNoteTool);
