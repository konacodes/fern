import { z, ZodSchema } from 'zod';
import type { Tool as AnthropicTool } from '@anthropic-ai/sdk/resources/messages';

// User context passed to tool execution
export interface UserContext {
  userId: string;
  name?: string;
  timezone?: string;
  preferences?: Record<string, unknown>;
  knowledge?: Record<string, unknown>;
}

// Result from tool execution
export interface ToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

// Tool definition interface
export interface Tool<TParams extends ZodSchema = ZodSchema> {
  name: string;
  description: string;
  parameters: TParams;
  execute: (params: z.infer<TParams>, userContext: UserContext) => Promise<ToolResult>;
}

// Internal registry for all tools
const toolRegistry = new Map<string, Tool>();

/**
 * Register a tool in the registry
 */
export function registerTool<TParams extends ZodSchema>(tool: Tool<TParams>): void {
  if (toolRegistry.has(tool.name)) {
    console.warn(`Tool "${tool.name}" is already registered. Overwriting.`);
  }
  toolRegistry.set(tool.name, tool as Tool);
}

/**
 * Get all registered tools
 */
export function getTools(): Tool[] {
  return Array.from(toolRegistry.values());
}

/**
 * Get a specific tool by name
 */
export function getTool(name: string): Tool | undefined {
  return toolRegistry.get(name);
}

/**
 * Convert Zod schema to JSON Schema for Anthropic API
 */
function zodToJsonSchema(schema: ZodSchema): Record<string, unknown> {
  // Get the shape of the schema
  const def = schema._def;

  if (def.typeName === 'ZodObject') {
    const shape = (schema as z.ZodObject<z.ZodRawShape>).shape;
    const properties: Record<string, unknown> = {};
    const required: string[] = [];

    for (const [key, value] of Object.entries(shape)) {
      const fieldSchema = value as ZodSchema;
      const fieldDef = fieldSchema._def;

      // Check if field is optional
      const isOptional = fieldDef.typeName === 'ZodOptional';
      const innerSchema = isOptional ? fieldDef.innerType : fieldSchema;
      const innerDef = innerSchema._def;

      if (!isOptional) {
        required.push(key);
      }

      // Convert the inner type
      properties[key] = zodFieldToJsonSchema(innerSchema, innerDef);
    }

    return {
      type: 'object',
      properties,
      required: required.length > 0 ? required : undefined,
    };
  }

  return { type: 'object', properties: {} };
}

function zodFieldToJsonSchema(schema: ZodSchema, def: z.ZodTypeDef & { typeName?: string; description?: string; values?: unknown[]; innerType?: ZodSchema; options?: ZodSchema[] }): Record<string, unknown> {
  const result: Record<string, unknown> = {};

  // Add description if present
  if (def.description) {
    result.description = def.description;
  }

  switch (def.typeName) {
    case 'ZodString':
      result.type = 'string';
      break;
    case 'ZodNumber':
      result.type = 'number';
      break;
    case 'ZodBoolean':
      result.type = 'boolean';
      break;
    case 'ZodArray':
      result.type = 'array';
      if (def.innerType) {
        result.items = zodFieldToJsonSchema(def.innerType, def.innerType._def);
      }
      break;
    case 'ZodEnum':
      result.type = 'string';
      result.enum = def.values;
      break;
    case 'ZodOptional':
      if (def.innerType) {
        return zodFieldToJsonSchema(def.innerType, def.innerType._def);
      }
      break;
    case 'ZodUnion':
      if (def.options) {
        result.oneOf = def.options.map((opt: ZodSchema) => zodFieldToJsonSchema(opt, opt._def));
      }
      break;
    default:
      result.type = 'string';
  }

  return result;
}

/**
 * Convert all registered tools to Anthropic API format
 */
export function getToolsForAnthropic(): AnthropicTool[] {
  return getTools().map((tool) => ({
    name: tool.name,
    description: tool.description,
    input_schema: zodToJsonSchema(tool.parameters) as AnthropicTool['input_schema'],
  }));
}

/**
 * Execute a tool by name with given parameters
 */
export async function executeTool(
  name: string,
  params: unknown,
  userContext: UserContext
): Promise<ToolResult> {
  const tool = getTool(name);

  if (!tool) {
    return {
      success: false,
      error: `Unknown tool: ${name}`,
    };
  }

  // Validate parameters against schema
  const parseResult = tool.parameters.safeParse(params);

  if (!parseResult.success) {
    return {
      success: false,
      error: `Invalid parameters: ${parseResult.error.message}`,
    };
  }

  try {
    return await tool.execute(parseResult.data, userContext);
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Tool execution failed',
    };
  }
}

/**
 * Clear all registered tools (useful for testing)
 */
export function clearTools(): void {
  toolRegistry.clear();
}
