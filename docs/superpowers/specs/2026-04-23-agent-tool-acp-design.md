# Agent-as-Tool with ACP Protocol Support

## Problem

Blazar currently has a sequential tool execution model with a basic `Tool` trait. The `AgentTool` exists but is tightly coupled to the internal sub-agent pattern. There is no standard protocol for communicating with external agents, no parallel tool execution, and the UI does not distinguish between local tools and remote agent invocations.

## Proposed Approach

**Approach A: Extended Tool Trait + Lightweight ACP Client** (chosen from three candidates).

Extend the existing `Tool` trait with agent awareness, resource declarations for parallel scheduling, and ACP protocol support — without replacing the current architecture. This follows Blazar's coding standards: own the product state, targeted adoption over framework takeover.

Build order: ACP protocol client → unified trait → resource-parallel scheduling → UI surface.

## Design

### 1. Unified Tool/Agent Trait Model

Extend `Tool` with optional methods for agent type, resource claims, and streaming.

```rust
/// Tool type: local synchronous vs ACP remote agent.
pub enum ToolKind {
    Local,
    AcpAgent { endpoint: String, agent_id: String },
}

/// Resource access declaration for parallel scheduling.
pub struct ResourceClaim {
    /// Resource identifier (e.g. "fs:src/main.rs", "process:bash", "network:api")
    pub resource_id: String,
    pub access: ResourceAccess,
}

pub enum ResourceAccess {
    /// Multiple ReadOnly claims on the same resource can run in parallel.
    ReadOnly,
    /// Mutually exclusive with any other claim on the same resource.
    ReadWrite,
    /// Mutually exclusive with all other tools (global lock).
    Exclusive,
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    fn kind(&self) -> ToolKind {
        ToolKind::Local
    }

    /// Declare which resources this call will access, given its arguments.
    /// Empty = no resource constraints = freely parallelizable.
    fn resource_claims(&self, args: &Value) -> Vec<ResourceClaim> {
        vec![]
    }

    fn execute(&self, args: Value) -> ToolResult;

    /// Optional streaming execution for ACP agents.
    fn execute_streaming(&self, _args: Value) -> Option<Box<dyn StreamingToolResult>> {
        None
    }
}

/// Iterator-style streaming result for ACP agents.
pub trait StreamingToolResult: Send {
    /// Returns the next content chunk, or None when the stream is complete.
    fn next_chunk(&mut self) -> Option<ContentPart>;
    /// Whether the overall result is an error (known after stream ends).
    fn is_error(&self) -> bool;
}
```

Key decisions:
- `resource_claims` takes `args` because resource dependencies depend on the specific call (e.g. `read_file("a.rs")` vs `read_file("b.rs")` claim different resources).
- Default `resource_claims` returns empty = freely parallelizable.
- Existing tools (read_file, bash, etc.) require zero changes — defaults cover them.

### 2. Resource-Based Parallel Scheduler

Conflict matrix for same-resource claims:

| Tool A \ Tool B | ReadOnly | ReadWrite | Exclusive |
|:--|:--|:--|:--|
| **ReadOnly** | ✅ parallel | ❌ serial | ❌ serial |
| **ReadWrite** | ❌ serial | ❌ serial | ❌ serial |
| **Exclusive** | ❌ serial | ❌ serial | ❌ serial |

```rust
pub struct ParallelScheduler;

impl ParallelScheduler {
    /// Partition pending tool calls into execution batches.
    /// Calls within a batch can run in parallel. Batches execute serially.
    pub fn schedule(
        calls: &[(String, &dyn Tool, Value)],
    ) -> Vec<Vec<usize>> {
        // 1. Collect resource_claims for each call
        // 2. Detect conflicts: same resource_id with incompatible access modes
        // 3. Non-conflicting calls go in the same batch
        // 4. Conflicting calls are pushed to the next batch
    }
}
```

Per-tool resource declarations:
- `ReadFileTool("src/a.rs")` → `[ResourceClaim("fs:src/a.rs", ReadOnly)]`
- `WriteFileTool("src/a.rs")` → `[ResourceClaim("fs:src/a.rs", ReadWrite)]`
- `BashTool` → `[ResourceClaim("process:bash", Exclusive)]` (conservative — bash can affect anything)
- `AcpAgentTool` → `[]` (remote agents have no local resource conflicts by default)

Execution in `turn.rs` changes from:
```
for each tool_call { execute(call); }
```
to:
```
batches = scheduler.schedule(calls);
for batch in batches {
    parallel_execute(batch);  // std::thread::scope or rayon
}
```

### 3. ACP Client

Each ACP agent is wrapped as a `Tool` implementation:

```rust
pub struct AcpAgentTool {
    agent_id: String,
    name: String,
    description: String,
    endpoint: String,
    input_schema: Value,
    http_client: reqwest::blocking::Client,
}
```

ACP REST mapping:

| Blazar operation | ACP endpoint | Notes |
|:--|:--|:--|
| List agents | `GET /agents` | Called at startup, registers into ToolRegistry |
| Agent details | `GET /agents/{id}` | Populates ToolSpec (name, description, parameters) |
| Sync execute | `POST /runs` + poll `GET /runs/{id}` | `Tool::execute` implementation |
| Streaming execute | `POST /runs` + SSE stream | `Tool::execute_streaming` implementation |
| Cancel | `POST /runs/{id}/cancel` | Maps to AgentCommand::Cancel |

Key handling:
- ACP MimeType content → translated into multi-modal `ToolResult` (see below).
- ACP async runs → `execute` internally polls until completion; synchronous to caller.
- Timeout and retry: reuse existing `MAX_TRANSIENT_RETRIES` strategy.
- Agent offline → `ToolResult::failure("agent unreachable: ...")`.

### 4. Agent Discovery and Configuration

#### 4a. Config file registration (priority)

```toml
# config/agents.toml

[[agents]]
name = "code-reviewer"
endpoint = "http://localhost:9100"
agent_id = "code-reviewer-v1"
enabled = true

[[agents]]
name = "search-agent"
endpoint = "http://remote-server:8080"
agent_id = "search"
enabled = true
```

At startup: read config → call `GET /agents/{id}` for each → register into ToolRegistry.

#### 4b. Runtime ACP discovery (supplement)

```rust
pub struct AcpDiscovery {
    endpoints: Vec<String>,
}

impl AcpDiscovery {
    pub fn discover(&self) -> Vec<AcpAgentInfo> {
        // GET /agents for each endpoint
        // Filter out agents already registered via config
        // Return newly discovered agents
    }
}
```

Discovery timing:
- At startup: load config agents first, then run discovery to supplement.
- User-triggered: via command (e.g. `/discover-agents`).
- No automatic polling (avoid unnecessary network overhead).

#### 4c. Multi-modal ToolResult

```rust
pub enum ContentPart {
    Text(String),
    Image { mime_type: String, data: Vec<u8> },
    Binary { mime_type: String, data: Vec<u8> },
}

pub struct ToolResult {
    pub content: Vec<ContentPart>,
    pub exit_code: Option<i32>,
    pub is_error: bool,
    pub output_truncated: bool,
}
```

Existing `ToolResult::success("text")` still works — wraps into `ContentPart::Text`.

Migration path: the current `output: String` field is replaced by `content: Vec<ContentPart>`. A helper method `ToolResult::text_output(&self) -> &str` provides backward-compatible access for code that only needs the text portion. All existing call sites that read `.output` are updated to use `.text_output()` or iterate `.content`.

### 5. UI Surface — Agent-Tool Awareness

#### 5a. AgentRuntimeState extension

```rust
pub struct AgentRuntimeState {
    pub turn_state: TurnState,
    pub turn_count: u64,
    pub streaming_text: String,
    /// Active tools — supports parallel execution (multiple concurrent).
    pub active_tools: Vec<ActiveToolInfo>,
    pub tool_call_count: u64,
}

pub struct ActiveToolInfo {
    pub call_id: String,
    pub tool_name: String,
    pub kind: ToolKind,
    pub status: ToolCallStatus,
}
```

#### 5b. Timeline entry enhancement

```rust
pub enum EntryKind {
    // ... existing variants unchanged ...
    ToolCall {
        call_id: String,
        tool_name: String,
        status: ToolCallStatus,
        kind: ToolKind,  // new: lets UI distinguish local tool vs ACP agent
    },
}
```

#### 5c. UI rendering

| Scenario | Display |
|:--|:--|
| Local tool (read_file) | `📁 read_file src/main.rs` — same as current |
| ACP agent call | `🤖 code-reviewer (ACP)` — agent badge + protocol marker |
| Parallel execution | Each tool shown **expanded** with its own status line simultaneously, e.g. `📁 read_file a.rs ⏳` and `🤖 code-reviewer (ACP) ⏳`, each updates to ✅/❌ on completion |
| Agent streaming response | Real-time streaming display, similar to main chat streaming |

Principles (per coding standards):
- All state lives in `AgentRuntimeState`, never in rendering layer.
- UI only reads `active_tools` and `timeline` data to render.
- Agent-tool awareness is informational, does not change operational flow.

## Error Handling

- ACP agent unreachable: `ToolResult::failure` with clear message, does not block other tools.
- Resource claim conflicts: scheduler pushes conflicting calls to next batch, never deadlocks.
- Streaming interruption: falls back to polling-based execution.
- Discovery failure: logged as warning, previously-configured agents still work.

## Testing Strategy

- Unit tests for `ParallelScheduler`: various conflict patterns, empty claims, all-exclusive.
- Unit tests for `AcpAgentTool::execute` with mock HTTP server.
- Integration test: register config agents + discovery agents, verify unified ToolRegistry.
- State tests: `AgentRuntimeState` transitions with parallel active_tools.
- Timeline rendering tests: verify ACP agent entries render distinctly from local tools.

## Implemented differences

- `ToolKind` shipped as `Local` or `Agent { is_acp: bool }` instead of storing endpoint and agent id inline. The ACP connection details stay on the concrete tool implementation; runtime/UI state only keeps the lightweight discriminator it needs.
- `ContentPart` shipped as text plus URI-backed resources (`Resource { uri, mime_type }`) rather than embedding image/binary payload bytes directly. This keeps ACP tool results cheaper to store and pass through the current blocking runtime.
- The initial ACP integration uses polling through `execute()` and runtime-triggered discovery refresh (`RefreshAcpAgents`) instead of a separate streaming tool trait. This matches the current synchronous tool runtime while still supporting startup registration plus explicit refresh.
