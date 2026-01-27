import { z } from 'zod';
import { Tool, registerTool, UserContext, ToolResult } from '../llm/tools.js';

// Parameters schema for WebSearch tool
const WebSearchParams = z.object({
  query: z.string().describe('The search query to look up'),
});

// Parameters schema for BrowseWeb tool
const BrowseWebParams = z.object({
  url: z.string().describe('The URL to browse'),
  task: z.string().describe('What to look for or do on the page'),
});

// Mock search results for stub implementation
const mockSearchResults = [
  {
    title: 'Example Search Result 1',
    url: 'https://example.com/result1',
    snippet: 'This is a mock search result. Real search integration coming soon.',
  },
  {
    title: 'Example Search Result 2',
    url: 'https://example.com/result2',
    snippet: 'Another mock result to demonstrate the search functionality.',
  },
  {
    title: 'Example Search Result 3',
    url: 'https://example.com/result3',
    snippet: 'Search results will come from a real search API in the future.',
  },
];

/**
 * WebSearch Tool - Search the web for current information
 *
 * NOTE: This is a stub implementation that returns mock results.
 * Future implementation will integrate with a search API (Brave, SerpAPI, etc.)
 */
export const WebSearchTool: Tool<typeof WebSearchParams> = {
  name: 'WebSearch',
  description: 'Search the web for current information. Returns a list of relevant search results.',
  parameters: WebSearchParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { query } = params;

    console.log(`[WebSearch] Searching for "${query}" (user: ${userContext.userId})`);

    // Return mock results with the query embedded
    const results = mockSearchResults.map((result, index) => ({
      ...result,
      title: `${result.title} for "${query}"`,
    }));

    return {
      success: true,
      data: {
        query,
        results,
        note: 'These are mock results. Web search integration coming soon!',
      },
    };
  },
};

/**
 * BrowseWeb Tool - Open a webpage and interact with it
 *
 * NOTE: This is a stub implementation for future Chrome MCP integration.
 * When implemented, this will use the Claude for Chrome MCP server to:
 * - Navigate to pages
 * - Extract content
 * - Fill forms
 * - Click buttons
 * - Handle complex web interactions
 */
export const BrowseWebTool: Tool<typeof BrowseWebParams> = {
  name: 'BrowseWeb',
  description: 'Open a webpage and interact with it. Can extract content, fill forms, and perform actions on web pages. Requires Chrome MCP integration (not yet implemented).',
  parameters: BrowseWebParams,

  execute: async (params, userContext: UserContext): Promise<ToolResult> => {
    const { url, task } = params;

    console.log(`[BrowseWeb] Would browse ${url} to: ${task} (user: ${userContext.userId})`);

    return {
      success: false,
      error: 'Browser automation is not yet implemented. This feature requires the Chrome MCP server integration which is planned for a future release.',
      data: {
        url,
        task,
        note: 'Chrome MCP integration coming soon!',
      },
    };
  },
};

// Register the tools
registerTool(WebSearchTool);
registerTool(BrowseWebTool);
