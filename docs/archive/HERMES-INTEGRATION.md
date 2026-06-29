# TDG-Rust Integration with Hermes-Agent: Hooks Architecture

**Date:** 2026-06-28
**Purpose:** Understand how Python TDG integrates with hermes-agent via hooks, so tdg-rust can support the same integration

---

## Executive Summary

Hermes-agent's memory provider system uses a **hook-based architecture** where providers are called at specific points in the conversation lifecycle. TDG (Python or Rust) can integrate as a memory provider that:

1. **Prefetches** relevant knowledge before each turn
2. **Syncs** new knowledge after each turn
3. **Exposes tools** for the agent to query/update the graph
4. **Handles compression** by extracting insights before context is summarized

The Rust TDG can support this via its MCP server — hermes-agent connects to it and the MCP tools map to the memory provider hooks.

---

## MemoryProvider Interface

### Core Methods (Required)

| Method | When Called | What It Does |
|--------|------------|--------------|
| `initialize(session_id, **kwargs)` | Agent startup | Connect to graph.db, warm up |
| `system_prompt_block()` | System prompt assembly | Static text for system prompt |
| `prefetch(query)` | Before each API call | Recall relevant context |
| `sync_turn(user, assistant)` | After each turn | Persist new knowledge |
| `get_tool_schemas()` | Tool registration | Expose tools to model |
| `handle_tool_call(name, args)` | Model calls tool | Dispatch tool calls |
| `shutdown()` | Agent exit | Flush, close connections |

### Optional Hooks

| Hook | When Called | What It Does |
|------|------------|--------------|
| `on_turn_start(turn, message)` | Start of each turn | Turn counting, scope management |
| `on_session_end(messages)` | Session boundary | End-of-session extraction |
| `on_session_switch(new_id)` | Session rotation | Update per-session state |
| `on_pre_compress(messages)` | Before compression | Extract insights from dying context |
| `on_memory_write(action, target, content)` | Built-in memory writes | Mirror to external store |
| `on_delegation(task, result)` | Subagent completes | Observe delegated work |
| `backup_paths()` | `hermes backup` | Extra paths to include |

---

## Hook Lifecycle (Conversation Flow)

```
User sends message
    │
    ├── on_turn_start(turn_number, message)
    │
    ├── prefetch(query) → inject into context
    │
    ├── [LLM call with context]
    │
    ├── sync_turn(user_msg, assistant_response)
    │
    ├── queue_prefetch(query) → for next turn
    │
    └── [next turn...]
    
Session boundary (exit/reset):
    ├── on_session_end(messages)
    └── shutdown()

Context compression:
    └── on_pre_compress(messages) → extract insights

Subagent completes:
    └── on_delegation(task, result)

Memory write (builtin):
    └── on_memory_write(action, target, content)
```

---

## How TDG Maps to Hooks

### Python TDG Integration

The Python TDG would implement a `MemoryProvider` subclass:

```python
class TDGMemoryProvider(MemoryProvider):
    name = "tdg"
    
    def initialize(self, session_id, **kwargs):
        self.db = GraphDB(kwargs['hermes_home'] / 'tdg' / 'graph.db')
        self.db.connect()
        self.retriever = HybridRetriever(self.db)
    
    def system_prompt_block(self):
        # Inject top memories into system prompt
        memories = self.db.get_top_memories(limit=20)
        return render_tiered_memories(memories)
    
    def prefetch(self, query):
        # Hybrid search for relevant context
        results = self.retriever.search(query, limit=10)
        return format_results(results)
    
    def sync_turn(self, user_msg, assistant_response):
        # Extract entities and create/update nodes
        entities = extract_entities(user_msg, assistant_response)
        for entity in entities:
            self.db.upsert_entity(entity)
    
    def get_tool_schemas(self):
        # Expose TDG tools to the model
        return [
            {"name": "tdg_search", ...},
            {"name": "tdg_create", ...},
            {"name": "tdg_update", ...},
        ]
    
    def handle_tool_call(self, tool_name, args):
        # Dispatch to TDG operations
        if tool_name == "tdg_search":
            return self.retriever.search(**args)
        elif tool_name == "tdg_create":
            return self.db.add_node(**args)
        # ...
    
    def on_pre_compress(self, messages):
        # Extract insights before compression
        insights = extract_insights(messages)
        for insight in insights:
            self.db.upsert_insight(insight)
        return format_insights(insights)
    
    def on_session_end(self, messages):
        # Extract session knowledge
        knowledge = extract_session_knowledge(messages)
        for k in knowledge:
            self.db.upsert_knowledge(k)
    
    def on_delegation(self, task, result):
        # Observe subagent work
        observation = {
            'task': task,
            'result': result,
            'timestamp': datetime.now().isoformat()
        }
        self.db.record_observation(observation)
```

### Rust TDG Integration (via MCP)

The Rust TDG doesn't need a Python `MemoryProvider` subclass. Instead, it runs as an MCP server and hermes-agent connects to it:

```yaml
# hermes config.yaml
mcp_servers:
  tdg:
    command: tdg-rust
    args: ["serve", "--port", "3001"]
```

The MCP tools map to the hooks:

| MemoryProvider Hook | TDG MCP Tool | What It Does |
|---------------------|--------------|--------------|
| `prefetch(query)` | `tdg_search` | Hybrid search for context |
| `sync_turn(user, assistant)` | `tdg_observe` | Auto-capture entities |
| `get_tool_schemas()` | 27 tools registered | All TDG tools exposed |
| `handle_tool_call()` | MCP dispatch | Tool calls routed to Rust |
| `on_pre_compress(messages)` | `tdg_probe` | Extract insights via HRR |
| `on_session_end(messages)` | `tdg_record_exec` | Record session execution |

---

## What tdg-rust Needs to Expose

### Current MCP Tools (27)

The Rust TDG already exposes all necessary tools via MCP. The key ones for memory integration:

| Tool | Purpose | Hook Equivalent |
|------|---------|-----------------|
| `tdg_search` | Hybrid FTS5 + vector search | `prefetch()` |
| `tdg_observe` | Auto-capture observations | `sync_turn()` |
| `tdg_create` | Create knowledge nodes | `sync_turn()` |
| `tdg_update` | Update node properties | `sync_turn()` |
| `tdg_connect` | Create edges | `sync_turn()` |
| `tdg_probe` | HRR compositional retrieval | `on_pre_compress()` |
| `tdg_hrr_related` | Find related concepts | `prefetch()` |
| `tdg_reason` | Infer relationships | `prefetch()` |
| `tdg_record_exec` | Record execution | `on_session_end()` |
| `tdg_self_manage` | Run maintenance | Background cron |

### What's Missing for Full Integration

The Rust TDG MCP server doesn't currently have a `prefetch` endpoint that hermes-agent can call automatically. The current architecture requires the LLM to explicitly call `tdg_search`.

**Solution:** Add a `tdg_prefetch` MCP tool that:
1. Takes a query string
2. Returns formatted context for injection
3. Is called automatically by hermes-agent's memory manager

```rust
#[tool]
pub async fn tdg_prefetch(
    Parameters(params): Parameters<PrefetchParams>,
) -> Result<String, McpError> {
    let query = params.query;
    let limit = params.limit.unwrap_or(10);
    
    let conn = get_conn(&self.pool)?;
    let retriever = HybridRetriever::new();
    let results = retriever.search(&conn, &query, limit)?;
    
    // Format results for context injection
    let context = results.iter()
        .map(|r| format!("[{}] {} — {}", r.node.node_type, r.node.name, r.node.description))
        .collect::<Vec<_>>()
        .join("\n");
    
    Ok(context)
}
```

---

## Integration Architecture

### Option A: Direct MCP Connection (Recommended)

```
Hermes-Agent
├── MemoryManager
│   ├── HonchoMemoryProvider (existing)
│   └── TDGMemoryProvider (NEW — thin wrapper)
│       ├── initialize() → connect to Rust TDG MCP server
│       ├── prefetch() → call tdg_search MCP tool
│       ├── sync_turn() → call tdg_observe MCP tool
│       ├── get_tool_schemas() → return TDG tools
│       └── handle_tool_call() → dispatch to MCP
└── MCP Connection
    └── Rust TDG Server (tdg-rust serve)
```

**Pros:** Clean separation, Rust handles all graph operations
**Cons:** Requires Python wrapper for the MemoryProvider interface

### Option B: Binary Integration

```
Hermes-Agent
├── MemoryManager
│   ├── HonchoMemoryProvider (existing)
│   └── TDGMemoryProvider (NEW)
│       ├── initialize() → call tdg-rust init
│       ├── prefetch() → call tdg-rust search
│       ├── sync_turn() → call tdg-rust observe
│       └── ...
└── Direct binary calls
    └── tdg-rust binary
```

**Pros:** No MCP overhead, direct binary calls
**Cons:** Tighter coupling, harder to deploy separately

### Option C: Hybrid (Best of Both)

```
Hermes-Agent
├── MemoryManager
│   ├── HonchoMemoryProvider (existing)
│   └── TDGMemoryProvider (NEW)
│       ├── initialize() → start Rust TDG as subprocess
│       ├── prefetch() → HTTP call to Rust TDG
│       ├── sync_turn() → HTTP call to Rust TDG
│       └── shutdown() → stop subprocess
└── HTTP Connection
    └── Rust TDG Server (tdg-rust serve --port 3001)
```

**Pros:** Process isolation, HTTP for easy debugging
**Cons:** Subprocess management overhead

---

## Recommended Approach: Option A

**Use MCP connection** because:
1. Hermes-agent already has MCP infrastructure
2. Rust TDG already has MCP server
3. No Python wrapper needed — MCP tools are directly callable
4. Clean separation of concerns

### Implementation Steps

1. **Add `tdg_prefetch` tool** to Rust TDG MCP server
2. **Create `TDGMemoryProvider`** in hermes-agent as thin MCP wrapper
3. **Configure `mcp_servers.tdg`** in hermes config
4. **Test integration** with real conversations

### TDGMemoryProvider Implementation

```python
# plugins/memory/tdg/__init__.py
class TDGMemoryProvider(MemoryProvider):
    name = "tdg"
    
    def __init__(self):
        self._mcp_client = None
        self._session_id = None
    
    def is_available(self) -> bool:
        # Check if tdg-rust binary is available
        return shutil.which("tdg-rust") is not None
    
    def initialize(self, session_id, **kwargs):
        self._session_id = session_id
        # Connect to Rust TDG MCP server
        self._mcp_client = MCPClient("tdg-rust", "serve")
    
    def system_prompt_block(self):
        return "<tdg-knowledge>Use tdg_search to query the knowledge graph.</tdg-knowledge>"
    
    def prefetch(self, query, *, session_id=""):
        # Call tdg_search via MCP
        result = self._mcp_client.call("tdg_search", {"query": query, "limit": 10})
        return result
    
    def sync_turn(self, user_content, assistant_content, **kwargs):
        # Call tdg_observe via MCP
        self._mcp_client.call("tdg_observe", {
            "description": f"User: {user_content[:200]}... Assistant: {assistant_content[:200]}..."
        })
    
    def get_tool_schemas(self):
        # Return all 27 TDG tools
        return self._mcp_client.list_tools()
    
    def handle_tool_call(self, tool_name, args, **kwargs):
        # Dispatch to Rust TDG
        return self._mcp_client.call(tool_name, args)
    
    def shutdown(self):
        if self._mcp_client:
            self._mcp_client.close()
```

---

## Key Differences: Python TDG vs Rust TDG Integration

| Aspect | Python TDG | Rust TDG |
|--------|-----------|----------|
| **Integration** | Direct Python import | MCP server connection |
| **MemoryProvider** | Native Python class | Thin MCP wrapper |
| **Tool dispatch** | Direct method calls | MCP protocol |
| **Startup** | Import module | Start binary |
| **Performance** | ~200ms | ~10ms |
| **Deployment** | pip install | Single binary |

---

## What tdg-rust Needs (Action Items)

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| **P0** | Add `tdg_prefetch` MCP tool | 1 day | Enables automatic context injection |
| **P1** | Create `TDGMemoryProvider` Python wrapper | 1 day | Enables hermes-agent integration |
| **P2** | Add `tdg_export`/`tdg_import` tools | 1 day | Enables data migration |
| **P2** | Add health check endpoint | 0.5 day | Enables monitoring |

---

## Files to Study

For deeper implementation guidance:
- `agent/memory_provider.py` — Full provider interface
- `agent/memory_manager.py` — Hook orchestration
- `plugins/memory/honcho/__init__.py` — Example provider
- `agent/conversation_compression.py` — Compression integration
- `agent/conversation_loop.py` — Hook call sites
