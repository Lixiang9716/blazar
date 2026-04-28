# Runtime Logic and TUI State Diagrams

This document maps current Blazar runtime flow and TUI-focused state transitions.

Code references:

- `src/chat/event_loop.rs`
- `src/chat/app.rs`
- `src/chat/app/actions.rs`
- `src/chat/app/turns.rs`
- `src/chat/app/events.rs`
- `src/chat/view/mod.rs`
- `src/chat/view/status.rs`
- `src/agent/runtime.rs`
- `src/agent/runtime/turn.rs`
- `src/agent/state.rs`

## 1) End-to-end runtime flow

```mermaid
flowchart TD
    A["run_terminal_chat()"] --> B["Init ChatApp + Terminal"]
    B --> C{"Main loop"}
    C --> D["app.tick()"]
    D --> E["drain agent_runtime.try_recv() -> apply_agent_event()"]
    E --> F["render_frame()"]
    F --> G{"event::poll/read"}
    G -->|key/mouse/paste| H["handle_action(InputAction)"]
    H --> C

    H -->|Submit| I["submit_composer()"]
    I --> J["send_message()"]
    J --> K{"agent_state.is_busy()"}
    K -->|yes| L["enqueue pending_messages"]
    K -->|no| M["dispatch_turn()"]
    M --> N["agent_runtime.submit_turn()"]

    N --> O["runtime_loop thread"]
    O --> P["run_turn_with_retry()"]
    P --> Q["execute_turn()"]
    Q --> R["stream_provider_pass()"]
    R --> S["ProviderEvent stream"]
    S --> T["ChannelObserver -> AgentEvent"]
    T --> D
```

## 2) TUI interaction/state graph (focus)

```mermaid
stateDiagram-v2
    [*] --> PickerClosed

    PickerClosed --> PickerOpen: open command/theme/model picker
    PickerOpen --> PickerClosed: Esc / close / select submit

    state "Users Status Mode" as USM {
        [*] --> Normal
        Normal --> CommandList: composer starts with "/"
        CommandList --> Normal: composer no longer starts with "/"
    }

    state "Streaming Indicator" as SI {
        [*] --> Off
        Off --> On: app.is_streaming() == true
        On --> Off: TurnComplete / TurnFailed
    }
```

## 3) Agent turn state machine (drives status label)

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Streaming: AgentEvent.TurnStarted
    Streaming --> Done: AgentEvent.TurnComplete
    Streaming --> Failed: AgentEvent.TurnFailed
    Done --> Streaming: next TurnStarted
    Failed --> Streaming: next TurnStarted

    state Streaming {
        [*] --> Thinking
        Thinking --> Executing: ToolCallStarted
        Executing --> Thinking: ToolCallCompleted
    }
```

## Rendering outputs

Rendered artifacts are generated from:

- `docs/development/diagrams/runtime-flow.mmd`
- `docs/development/diagrams/tui-state.mmd`
- `docs/development/diagrams/agent-turn-state.mmd`

Into:

- `docs/development/diagrams/runtime-flow.svg`
- `docs/development/diagrams/tui-state.svg`
- `docs/development/diagrams/agent-turn-state.svg`
