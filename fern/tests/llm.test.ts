import { describe, it, expect, beforeEach } from 'vitest';
import { z } from 'zod';
import {
  registerTool,
  getTools,
  getTool,
  getToolsForAnthropic,
  executeTool,
  clearTools,
  type Tool,
  type UserContext,
} from '../src/llm/tools.js';
import {
  buildSystemPrompt,
  buildOnboardingPrompt,
  buildUnknownUserPrompt,
  type PromptUserContext,
} from '../src/llm/prompts.js';

describe('Tool Registry', () => {
  beforeEach(() => {
    clearTools();
  });

  it('should register and retrieve a tool', () => {
    const testTool: Tool = {
      name: 'test_tool',
      description: 'A test tool',
      parameters: z.object({
        message: z.string(),
      }),
      execute: async (params) => ({
        success: true,
        data: `Received: ${params.message}`,
      }),
    };

    registerTool(testTool);

    const retrieved = getTool('test_tool');
    expect(retrieved).toBeDefined();
    expect(retrieved?.name).toBe('test_tool');
    expect(retrieved?.description).toBe('A test tool');
  });

  it('should return all registered tools', () => {
    const tool1: Tool = {
      name: 'tool1',
      description: 'First tool',
      parameters: z.object({}),
      execute: async () => ({ success: true }),
    };

    const tool2: Tool = {
      name: 'tool2',
      description: 'Second tool',
      parameters: z.object({}),
      execute: async () => ({ success: true }),
    };

    registerTool(tool1);
    registerTool(tool2);

    const tools = getTools();
    expect(tools).toHaveLength(2);
    expect(tools.map((t) => t.name)).toContain('tool1');
    expect(tools.map((t) => t.name)).toContain('tool2');
  });

  it('should overwrite tool with same name', () => {
    const tool1: Tool = {
      name: 'same_name',
      description: 'First version',
      parameters: z.object({}),
      execute: async () => ({ success: true }),
    };

    const tool2: Tool = {
      name: 'same_name',
      description: 'Second version',
      parameters: z.object({}),
      execute: async () => ({ success: true }),
    };

    registerTool(tool1);
    registerTool(tool2);

    const tools = getTools();
    expect(tools).toHaveLength(1);
    expect(tools[0].description).toBe('Second version');
  });

  it('should return undefined for unknown tool', () => {
    expect(getTool('nonexistent')).toBeUndefined();
  });
});

describe('Tool Execution', () => {
  beforeEach(() => {
    clearTools();
  });

  const mockUserContext: UserContext = {
    userId: 'user123',
    name: 'Test User',
    timezone: 'America/New_York',
  };

  it('should execute tool successfully', async () => {
    const echoTool: Tool = {
      name: 'echo',
      description: 'Echoes the input',
      parameters: z.object({
        message: z.string(),
      }),
      execute: async (params) => ({
        success: true,
        data: params.message,
      }),
    };

    registerTool(echoTool);

    const result = await executeTool('echo', { message: 'hello' }, mockUserContext);
    expect(result.success).toBe(true);
    expect(result.data).toBe('hello');
  });

  it('should return error for unknown tool', async () => {
    const result = await executeTool('unknown', {}, mockUserContext);
    expect(result.success).toBe(false);
    expect(result.error).toContain('Unknown tool');
  });

  it('should return error for invalid parameters', async () => {
    const strictTool: Tool = {
      name: 'strict',
      description: 'Requires specific params',
      parameters: z.object({
        required: z.string(),
        count: z.number(),
      }),
      execute: async () => ({ success: true }),
    };

    registerTool(strictTool);

    const result = await executeTool('strict', { required: 'yes' }, mockUserContext);
    expect(result.success).toBe(false);
    expect(result.error).toContain('Invalid parameters');
  });

  it('should handle execution errors gracefully', async () => {
    const errorTool: Tool = {
      name: 'error_tool',
      description: 'Always throws',
      parameters: z.object({}),
      execute: async () => {
        throw new Error('Intentional error');
      },
    };

    registerTool(errorTool);

    const result = await executeTool('error_tool', {}, mockUserContext);
    expect(result.success).toBe(false);
    expect(result.error).toBe('Intentional error');
  });

  it('should pass user context to tool', async () => {
    let receivedContext: UserContext | null = null;

    const contextTool: Tool = {
      name: 'context_tool',
      description: 'Captures context',
      parameters: z.object({}),
      execute: async (_, ctx) => {
        receivedContext = ctx;
        return { success: true };
      },
    };

    registerTool(contextTool);

    await executeTool('context_tool', {}, mockUserContext);
    expect(receivedContext).toEqual(mockUserContext);
  });
});

describe('Anthropic Tool Conversion', () => {
  beforeEach(() => {
    clearTools();
  });

  it('should convert simple tool to Anthropic format', () => {
    const simpleTool: Tool = {
      name: 'simple',
      description: 'A simple tool',
      parameters: z.object({
        name: z.string(),
        count: z.number(),
      }),
      execute: async () => ({ success: true }),
    };

    registerTool(simpleTool);

    const anthropicTools = getToolsForAnthropic();
    expect(anthropicTools).toHaveLength(1);
    expect(anthropicTools[0].name).toBe('simple');
    expect(anthropicTools[0].description).toBe('A simple tool');
    expect(anthropicTools[0].input_schema).toEqual({
      type: 'object',
      properties: {
        name: { type: 'string' },
        count: { type: 'number' },
      },
      required: ['name', 'count'],
    });
  });

  it('should handle optional parameters', () => {
    const optionalTool: Tool = {
      name: 'optional',
      description: 'Has optional params',
      parameters: z.object({
        required: z.string(),
        optional: z.string().optional(),
      }),
      execute: async () => ({ success: true }),
    };

    registerTool(optionalTool);

    const anthropicTools = getToolsForAnthropic();
    const schema = anthropicTools[0].input_schema;
    expect(schema.required).toEqual(['required']);
  });

  it('should handle enum parameters', () => {
    const enumTool: Tool = {
      name: 'enum_tool',
      description: 'Has enum param',
      parameters: z.object({
        status: z.enum(['pending', 'completed', 'cancelled']),
      }),
      execute: async () => ({ success: true }),
    };

    registerTool(enumTool);

    const anthropicTools = getToolsForAnthropic();
    const props = anthropicTools[0].input_schema.properties as Record<string, { type: string; enum?: string[] }>;
    expect(props.status.type).toBe('string');
    expect(props.status.enum).toEqual(['pending', 'completed', 'cancelled']);
  });
});

describe('System Prompts', () => {
  it('should build system prompt with user context', () => {
    const userContext: PromptUserContext = {
      name: 'Alice',
      timezone: 'America/Los_Angeles',
      knowledge: {
        favoriteColor: 'blue',
        occupation: 'engineer',
      },
    };

    const prompt = buildSystemPrompt(userContext);

    expect(prompt).toContain('Fern');
    expect(prompt).toContain('Alice');
    expect(prompt).toContain('America/Los_Angeles');
    expect(prompt).toContain('favoriteColor');
    expect(prompt).toContain('blue');
    expect(prompt).toContain('occupation');
    expect(prompt).toContain('engineer');
  });

  it('should handle empty user context', () => {
    const prompt = buildSystemPrompt({});

    expect(prompt).toContain('friend'); // Default name
    expect(prompt).toContain('unknown'); // Unknown timezone
    expect(prompt).toContain('Not much yet'); // Empty knowledge
    expect(prompt).toContain('None currently'); // No reminders
  });

  it('should format pending reminders', () => {
    const userContext: PromptUserContext = {
      name: 'Bob',
      pendingReminders: [
        {
          id: '1',
          userId: 'user1',
          message: 'Call mom',
          triggerType: 'time',
          triggerValue: '2024-01-15T17:00:00Z',
          status: 'pending',
          createdAt: new Date(),
          triggeredAt: null,
        },
        {
          id: '2',
          userId: 'user1',
          message: 'Buy groceries',
          triggerType: 'condition',
          triggerValue: 'when I get home',
          status: 'pending',
          createdAt: new Date(),
          triggeredAt: null,
        },
      ],
    };

    const prompt = buildSystemPrompt(userContext);

    expect(prompt).toContain('Call mom');
    expect(prompt).toContain('Buy groceries');
    expect(prompt).toContain('when I get home');
  });

  it('should build onboarding prompt', () => {
    const prompt = buildOnboardingPrompt();

    expect(prompt).toContain('Fern');
    expect(prompt).toContain('welcome');
    expect(prompt).toContain('call you');
  });

  it('should build unknown user prompt', () => {
    const prompt = buildUnknownUserPrompt();

    expect(prompt).toContain('Fern');
    expect(prompt).toContain('magic word');
  });
});
