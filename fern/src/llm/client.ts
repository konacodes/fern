import Anthropic from '@anthropic-ai/sdk';
import type {
  MessageParam,
  ContentBlock,
  ToolUseBlock,
  ToolResultBlockParam,
  TextBlock,
} from '@anthropic-ai/sdk/resources/messages';
import { getToolsForAnthropic, executeTool, type UserContext, type ToolResult } from './tools.js';

// Re-export types for convenience
export type { MessageParam, ContentBlock };

// Configuration for the LLM client
export interface LLMConfig {
  apiKey?: string;
  model?: string;
  maxTokens?: number;
}

// Chat response with all messages generated
export interface ChatResponse {
  messages: string[];
  toolCalls: ToolCallInfo[];
  stopReason: string;
}

// Information about a tool call
export interface ToolCallInfo {
  name: string;
  input: unknown;
  result: ToolResult;
}

// Callback for streaming text chunks
export type OnTextChunk = (text: string) => void;

// Callback for when a tool is about to be called
export type OnToolStart = (name: string, input: unknown) => void;

// Callback for when a tool completes
export type OnToolComplete = (name: string, result: ToolResult) => void;

// Streaming callbacks
export interface StreamCallbacks {
  onTextChunk?: OnTextChunk;
  onToolStart?: OnToolStart;
  onToolComplete?: OnToolComplete;
}

// Default model to use
const DEFAULT_MODEL = 'claude-sonnet-4-20250514';
const DEFAULT_MAX_TOKENS = 1024;

/**
 * LLM Client - Wrapper around Anthropic API with streaming and tool support
 */
export class LLMClient {
  private client: Anthropic;
  private model: string;
  private maxTokens: number;

  constructor(config: LLMConfig = {}) {
    this.client = new Anthropic({
      apiKey: config.apiKey || process.env.ANTHROPIC_API_KEY,
    });
    this.model = config.model || DEFAULT_MODEL;
    this.maxTokens = config.maxTokens || DEFAULT_MAX_TOKENS;
  }

  /**
   * Chat with the LLM, handling streaming and tool calls
   */
  async chat(
    systemPrompt: string,
    messages: MessageParam[],
    userContext: UserContext,
    callbacks?: StreamCallbacks
  ): Promise<ChatResponse> {
    const tools = getToolsForAnthropic();
    const responseMessages: string[] = [];
    const toolCalls: ToolCallInfo[] = [];
    let currentMessages = [...messages];

    // Keep going until we get a final response (not a tool call)
    let continueLoop = true;

    while (continueLoop) {
      const response = await this.streamMessage(
        systemPrompt,
        currentMessages,
        tools,
        callbacks
      );

      // Collect text content
      const textContent = response.content
        .filter((block): block is TextBlock => block.type === 'text')
        .map((block) => block.text)
        .join('');

      if (textContent) {
        responseMessages.push(textContent);
      }

      // Check for tool use
      const toolUseBlocks = response.content.filter(
        (block): block is ToolUseBlock => block.type === 'tool_use'
      );

      if (toolUseBlocks.length > 0 && response.stop_reason === 'tool_use') {
        // Execute all tool calls
        const toolResults: ToolResultBlockParam[] = [];

        for (const toolUse of toolUseBlocks) {
          callbacks?.onToolStart?.(toolUse.name, toolUse.input);

          const result = await executeTool(
            toolUse.name,
            toolUse.input,
            userContext
          );

          toolCalls.push({
            name: toolUse.name,
            input: toolUse.input,
            result,
          });

          callbacks?.onToolComplete?.(toolUse.name, result);

          toolResults.push({
            type: 'tool_result',
            tool_use_id: toolUse.id,
            content: JSON.stringify(result),
          });
        }

        // Add assistant message with tool use and tool results to continue conversation
        currentMessages = [
          ...currentMessages,
          { role: 'assistant', content: response.content },
          { role: 'user', content: toolResults },
        ];
      } else {
        // No more tool calls, we're done
        continueLoop = false;
      }
    }

    return {
      messages: responseMessages,
      toolCalls,
      stopReason: 'end_turn',
    };
  }

  /**
   * Stream a single message from the LLM
   */
  private async streamMessage(
    systemPrompt: string,
    messages: MessageParam[],
    tools: Anthropic.Tool[],
    callbacks?: StreamCallbacks
  ): Promise<Anthropic.Message> {
    const stream = this.client.messages.stream({
      model: this.model,
      max_tokens: this.maxTokens,
      system: systemPrompt,
      messages,
      tools: tools.length > 0 ? tools : undefined,
    });

    // Handle streaming text chunks
    stream.on('text', (text) => {
      callbacks?.onTextChunk?.(text);
    });

    // Wait for the final message
    const finalMessage = await stream.finalMessage();

    return finalMessage;
  }

  /**
   * Simple non-streaming chat for quick responses
   */
  async chatSimple(
    systemPrompt: string,
    messages: MessageParam[],
    userContext: UserContext
  ): Promise<string> {
    const response = await this.chat(systemPrompt, messages, userContext);
    return response.messages.join('\n');
  }
}

// Singleton instance for convenience
let defaultClient: LLMClient | null = null;

/**
 * Get or create the default LLM client
 */
export function getLLMClient(config?: LLMConfig): LLMClient {
  if (!defaultClient || config) {
    defaultClient = new LLMClient(config);
  }
  return defaultClient;
}
