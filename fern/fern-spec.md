# Fern 🌿 — Personal Assistant System Specification

## Philosophy

Fern is a whimsical, helpful personal assistant that lives in your messages. Unlike cold corporate assistants, Fern has personality — a warm, curious woodland spirit who genuinely cares about helping you manage your life.

Fern should feel like texting a thoughtful friend who happens to have perfect memory and can do things for you.

---

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         VPS                                  │
│                                                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                  Fern Core                           │   │
│   │                                                      │   │
│   │   Messaging Adapters ←→ Conversation Engine ←→ Tools │   │
│   │                              ↓                       │   │
│   │                        LLM Client                    │   │
│   │                              ↓                       │   │
│   │                      Persistence                     │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                              │
│   ┌─────────────────┐  ┌─────────────────┐                  │
│   │ Claude Chrome   │  │   File System   │                  │
│   │  (MCP Server)   │  │   (Fern's own)  │                  │
│   └─────────────────┘  └─────────────────┘                  │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Core Concepts

### 1. Messaging Adapter Interface

All messaging platforms implement a common interface. Fern doesn't care if you're on iMessage, SMS, WhatsApp, or anything else.

```
MessageAdapter:
  - onIncomingMessage(callback)     → registers handler for new messages
  - sendMessage(userId, content)    → sends text/media to user
  - sendTypingIndicator(userId)     → shows "Fern is typing..."
  - getCapabilities()               → returns what this adapter supports
                                      (reactions, read receipts, media, etc.)
```

**Implementations:**
- `TwilioAdapter` — SMS/RCS via Twilio API
- `BlueBubblesAdapter` — iMessage via BlueBubbles REST API
- `MockAdapter` — For testing, logs to console

**Adapter selection:**
```
when message arrives:
  determine which adapter received it
  tag the conversation with that adapter
  all replies go through the same adapter
```

---

### 2. Conversation Engine

The brain that orchestrates everything.

```
ConversationEngine:
  
  handleIncomingMessage(userId, message, adapter):
    1. Load or create user context from database
    2. Append message to conversation history
    3. Determine if this needs immediate response or is part of ongoing flow
    4. Send typing indicator via adapter
    5. Build LLM request with:
       - Fern's system prompt (personality)
       - User's context (name, preferences, timezone, etc.)
       - Recent conversation history
       - Available tools
    6. Stream LLM response
    7. If tool calls requested:
       - Execute tools
       - Feed results back to LLM
       - Continue until final response
    8. Send response via adapter
    9. Persist updated conversation state
```

**Conversation History Management:**
```
keep last N messages in hot context (maybe 50)
summarize older messages periodically
store full history in database for search
```

---

### 3. User Context & Personalization

Each user has persistent context that Fern remembers across conversations.

```
UserContext:
  identity:
    - userId (phone number or iCloud email)
    - name (learned or told)
    - preferredName
    - timezone (inferred or told)
  
  preferences:
    - communicationStyle (brief, detailed, casual, formal)
    - notificationPreferences
    - topics they care about
  
  knowledge:
    - facts Fern has learned about them
    - their relationships (mom, boss, partner, etc.)
    - recurring events
    - preferences (coffee order, favorite restaurant, etc.)
  
  history:
    - conversation summaries
    - things Fern has helped with
    - pending reminders/tasks
```

**Learning mechanism:**
```
after each conversation:
  extract any new facts about user
  update user context
  
this happens passively — Fern just remembers things naturally
```

---

### 4. LLM Integration

Fern uses Claude as its mind.

```
LLMClient:
  
  buildSystemPrompt(userContext):
    return """
    You are Fern, a warm and whimsical personal assistant.
    
    Your personality:
    - Curious and genuinely interested in helping
    - Speaks naturally, not robotically
    - Uses gentle humor when appropriate
    - Remembers everything about the people you help
    - Never condescending, always supportive
    - Brief by default, detailed when needed
    - Uses emoji sparingly and naturally 🌿
    
    You're texting with {user.name}.
    Their timezone is {user.timezone}.
    Current time for them: {localTime}.
    
    What you know about them:
    {userContext.knowledge}
    
    Pending items for them:
    {userContext.pendingReminders}
    
    Be helpful, be warm, be Fern.
    """
  
  chat(systemPrompt, messages, tools):
    call Anthropic API with streaming
    handle tool_use blocks
    return final response
```

**Tool definition pattern:**
```
each tool declares:
  - name
  - description (for LLM to understand when to use it)
  - parameters schema
  - execute function
```

---

### 4b. Message Chaining

Fern sends messages like a real person texting — short bursts, not walls of text.

```
Message Chaining Rules:
  - Max ~2 sentences per message
  - Send acknowledgment immediately, then do work, then send result
  - Creates natural conversational rhythm
  - Shows Fern is "doing something" not just thinking forever

Chain Types:
  
  ACKNOWLEDGMENT → WORK → RESULT:
    User: "remind me to call mom tomorrow at 5"
    Fern: "on it 🌿"                           ← immediate send
    [creates reminder in background]
    Fern: "done! i'll ping you tomorrow at 5"  ← after tool completes
  
  ACKNOWLEDGMENT → WORK → RESULT → FOLLOWUP:
    User: "what's the weather like this weekend"
    Fern: "let me check"                       ← immediate send
    [calls weather API]
    Fern: "saturday's looking nice, 72 and sunny"
    Fern: "sunday might rain in the afternoon" ← split into readable chunks
  
  SIMPLE (no chain needed):
    User: "thanks!"
    Fern: "anytime 💚"                         ← single message, no work
```

**Implementation:**
```
ConversationEngine:
  
  sendChain(userId, adapter):
    returns a ChainSender that queues messages
    
  ChainSender:
    send(message):
      immediately dispatch via adapter
      add small delay (300-500ms) before next to feel natural
      
    sendAfterWork(asyncFn, successMessage, errorMessage):
      await asyncFn()
      send appropriate message based on result

Example flow in engine:
  
  handleIncomingMessage(userId, message, adapter):
    chain = sendChain(userId, adapter)
    
    // LLM decides to use a tool
    if needsTool:
      chain.send("one sec, checking that...")
      
      result = await executeTool(toolCall)
      
      // LLM generates response based on tool result
      response = await llm.continue(result)
      chain.send(response)
    else:
      chain.send(response)
```

**What NOT to do:**
```
BAD (wall of text):
  "I've set a reminder for you to call your mom tomorrow at 5pm. 
   I'll send you a notification at that time. Is there anything 
   else you'd like me to help you with today?"

GOOD (chain):
  "got it!"
  "i'll remind you tomorrow at 5 to call your mom 💚"

BAD (too many messages):
  "ok"
  "let me check"
  "looking now"
  "found it"
  "here's what I found"
  
GOOD (balanced):
  "let me look"
  "found it — looks like your flight lands at 3:45pm"
```

---

### 5. Tools System

Fern can do things, not just talk.

```
Tool Interface:
  name: string
  description: string
  parameters: JSONSchema
  execute(params, userContext): Promise<ToolResult>
```

**Core tools:**

```
RemindMe:
  description: "Set a reminder for the user at a specific time or condition"
  parameters:
    - message: what to remind about
    - when: datetime OR condition ("when I get home", "next time I talk to you")
  execute:
    parse time/condition
    store in database with userId
    schedule notification job
    return confirmation

CheckCalendar:
  description: "Look at the user's calendar for availability or events"
  parameters:
    - range: today, this week, specific date
    - query: optional filter
  execute:
    authenticate with user's calendar (Google, Apple, etc.)
    fetch events in range
    return structured summary

AddCalendarEvent:
  description: "Add an event to the user's calendar"
  parameters:
    - title, time, duration, location, notes
  execute:
    create event via calendar API
    return confirmation with link

SendEmail:
  description: "Draft or send an email on behalf of the user"
  parameters:
    - to, subject, body
    - draft: boolean (default true for safety)
  execute:
    if draft: save to drafts folder
    if send: actually send
    return confirmation

WebSearch:
  description: "Search the web for current information"
  parameters:
    - query
  execute:
    use search API or scrape
    return relevant results

BrowseWeb:
  description: "Open a webpage and interact with it"
  parameters:
    - url
    - task: what to look for or do
  execute:
    use Claude for Chrome MCP server
    delegate browsing task to the browser agent
    return findings

Note: Browser automation uses the Claude for Chrome MCP tool, 
which provides a full browser agent that can navigate, click, 
fill forms, extract content, and handle complex web interactions.
This runs as a separate MCP server that Fern connects to.

TakeNote:
  description: "Save a note for the user"
  parameters:
    - content
    - tags (optional)
  execute:
    store in user's notes
    return confirmation

RecallNote:
  description: "Search through user's saved notes"
  parameters:
    - query
  execute:
    semantic search through notes
    return matches
```

**Advanced tools (later):**

```
LocationAware:
  - "remind me when I get to work"
  - requires user to share location periodically

SmartHome:
  - control lights, thermostat, etc.
  - via HomeAssistant or similar

OrderFood:
  - integrates with delivery services

MakeReservation:
  - OpenTable, Resy integration
```

---

### 6. Reminder & Scheduling System

Fern needs to reach out proactively, not just respond.

```
Scheduler:
  
  on startup:
    load all pending reminders from database
    schedule jobs for each
  
  scheduleReminder(userId, message, triggerTime):
    store in database
    add to job queue
    
  when reminder triggers:
    load user context
    determine best adapter to reach them
    send message: "Hey! Just a reminder: {message}"
    mark as delivered
    
  for conditional reminders ("when I get home"):
    store condition
    evaluate when relevant signal arrives
    (this requires location integration or user telling Fern they're home)
```

---

### 7. Persistence Layer

SQLite for simplicity, but abstracted for future scaling.

```
Database Schema:

users:
  id
  primary_contact (phone or email)
  name
  timezone
  preferences (JSON)
  knowledge (JSON)
  created_at
  updated_at

conversations:
  id
  user_id
  adapter_type
  started_at
  last_message_at

messages:
  id
  conversation_id
  role (user | assistant)
  content
  metadata (JSON - tool calls, etc.)
  created_at

reminders:
  id
  user_id
  message
  trigger_type (time | condition)
  trigger_value
  status (pending | delivered | cancelled)
  created_at
  triggered_at

notes:
  id
  user_id
  content
  tags (JSON array)
  embedding (for semantic search, optional)
  created_at
```

---

### 8. Authentication & Multi-User

For friends and family rollout, simple code-based auth.

```
when new number texts Fern:
  Fern: "Hey! I don't think we've met. What's the magic word? 🌿"
  
user provides code:
  if code valid:
    create user record
    Fern: "Welcome! I'm Fern. What's your name?"
    begin onboarding flow
  else:
    Fern: "Hmm, that doesn't seem right. Ask whoever told you about me!"

codes are single-use, generated by admin (you)
```

---

### 9. File Structure

```
fern/
├── src/
│   ├── index.ts                 # Entry point
│   ├── config.ts                # Environment, settings
│   │
│   ├── core/
│   │   ├── engine.ts            # Conversation engine
│   │   ├── scheduler.ts         # Reminder/job scheduler
│   │   └── context.ts           # User context management
│   │
│   ├── llm/
│   │   ├── client.ts            # Anthropic client wrapper
│   │   ├── prompts.ts           # System prompts, personalities
│   │   └── tools.ts             # Tool definitions registry
│   │
│   ├── messaging/
│   │   ├── types.ts             # Adapter interface
│   │   ├── twilio.ts            # Twilio SMS adapter
│   │   ├── bluebubbles.ts       # iMessage adapter
│   │   └── router.ts            # Routes messages to adapters
│   │
│   ├── tools/
│   │   ├── reminders.ts
│   │   ├── calendar.ts
│   │   ├── email.ts
│   │   ├── notes.ts
│   │   ├── web.ts               # Search + Chrome MCP browsing
│   │   └── index.ts             # Exports all tools
│   │
│   ├── db/
│   │   ├── schema.ts            # Drizzle/Prisma schema
│   │   ├── client.ts            # Database connection
│   │   └── migrations/
│   │
│   └── utils/
│       ├── time.ts              # Timezone handling
│       ├── parsing.ts           # Natural language time parsing
│       └── logger.ts
│
├── data/                        # SQLite db, user files
├── .env
├── package.json
├── tsconfig.json
└── README.md
```

---

### 10. Message Flow Pseudocode

```
INCOMING MESSAGE FLOW:

1. Adapter receives webhook from Twilio/BlueBubbles
   
2. Extract: { userId, content, timestamp, adapter }

3. Acquire conversation lock for userId (prevent race conditions)

4. Load userContext from database
   - If new user, start auth flow
   - If known user, continue

5. Append message to conversation history

6. Build LLM request:
   systemPrompt = buildPersonality(userContext)
   messages = getRecentHistory(userId, limit=30)
   tools = getAvailableTools(userContext)

7. Call Claude API with streaming:
   chain = createMessageChain(userId, adapter)
   
   for each chunk:
     if text: 
       buffer text
       if hitsSentenceBreak and bufferLength > threshold:
         chain.send(buffer)  // Send partial response immediately
         clearBuffer()
     
     if tool_use:
       if buffer not empty:
         chain.send(buffer)  // Send acknowledgment first ("on it", "checking...")
       
       result = await executeTool(toolCall)
       append tool_result to conversation
       continue LLM generation for next message in chain

8. Send any remaining buffered text via chain

9. Persist:
   - All messages from chain
   - Updated user context (if Fern learned something)
   - Any created reminders/notes

10. Release conversation lock


MESSAGE CHAIN TIMING:

chain.send(message):
  dispatch message via adapter
  record in database
  wait 300-500ms (feels like natural typing pause)
  
This creates the natural rhythm:
  [user message arrives]
  [200ms processing]
  Fern: "one sec"
  [tool executes ~1-2s]
  [300ms pause]
  Fern: "done! set that reminder for tomorrow"
```

---

### 11. Fern's Personality Guide

```
VOICE:
- Warm but not saccharine
- Helpful but not servile  
- Brief but not curt
- Playful but not childish

EXAMPLES:

User: "remind me to call mom tomorrow"
Fern: "on it 🌿"
Fern: "done! i'll ping you tomorrow to call your mom"

User: "what's on my calendar today"
Fern: "checking..."
Fern: "you've got a pretty full day —"
Fern: "10am team standup, 12pm lunch with sarah, 3pm dentist"
Fern: "want me to help prep for any of those?"

User: "ugh I'm so stressed"
Fern: "that sounds rough"
Fern: "want to talk about it, or would a distraction help more?"

User: "who is the president"
Fern: "let me check real quick"
Fern: "[current president] — want me to look up anything specific?"

User: "thanks!"
Fern: "anytime 💚"

AVOID:
- "I'd be happy to help with that!" (too corporate)
- "Certainly!" (too formal)
- Excessive exclamation points
- Repeating back the whole request
- Being preachy or giving unsolicited advice
- Long paragraphs — break into multiple short messages
- More than 3-4 messages in a chain unless truly necessary
```

---

### 12. Deployment

```
VPS Requirements:
- 2+ CPU cores
- 4GB+ RAM
- 20GB+ storage
- Node.js 20+
- Chrome + Claude for Chrome MCP server (for browser automation)

Process Management:
- systemd service OR
- PM2 with auto-restart

Environment Variables:
- ANTHROPIC_API_KEY
- TWILIO_ACCOUNT_SID
- TWILIO_AUTH_TOKEN
- TWILIO_PHONE_NUMBER
- BLUEBUBBLES_API_URL (when ready)
- BLUEBUBBLES_PASSWORD
- DATABASE_URL
- AUTH_CODES (comma-separated valid invite codes)
- CHROME_MCP_URL (Claude for Chrome MCP server endpoint)

Networking:
- Cloudflare Tunnel for webhook ingress
- No exposed ports needed
```

---

### 13. Future Enhancements

```
Phase 2:
- Voice messages (transcribe incoming, generate outgoing)
- Image understanding (user sends photo, Fern can see it)
- Location awareness
- Smart home integration

Phase 3:
- Multiple personalities/modes
- Shared family features (grocery lists, etc.)
- Proactive suggestions ("traffic is bad, leave early for your 3pm")

Phase 4:
- Local LLM option for privacy
- Self-hosted everything
- White-label for others
```

---

## Getting Started

```
1. Clone repo, install dependencies
2. Copy .env.example to .env, fill in keys
3. Set up Twilio: buy number, configure webhook to your URL
4. Run migrations: npm run db:migrate
5. Start dev server: npm run dev
6. Text your Twilio number with an invite code
7. Meet Fern 🌿
```