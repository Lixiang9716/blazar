# Context Window Usage via Provider API Design

## 1. Problem

Blazar UI already has a `context_usage` display slot (`used/max (%)`), but runtime does not populate it from real provider usage data.  
Current value is mostly test-injected, so users cannot trust the context window indicator during real turns.

## 2. Goals

1. Use provider-returned usage as the primary source of truth for `used_tokens`.
2. Support both OpenAI-compatible provider and OpenRouter provider.
3. Resolve `max_tokens` with priority:
   1. real model context length
   2. fallback to `config/provider.json` `max_tokens`
4. Keep existing UX format in model row (`used/max (percent%)`) and show `n/a` when max is unavailable.

## 3. Non-Goals

1. No tokenizer-based local estimation in this phase.
2. No redesign of timeline/message rendering.
3. No new UI panel; only wire existing model-row usage display.

## 4. External API Findings

### 4.1 OpenAI-compatible (Chat Completions)

Official documented OpenAPI spec indicates:

1. Streaming usage is exposed via `stream_options.include_usage`.
2. Final chunk may contain usage with empty `choices`.
3. Intermediate chunks can have `usage: null`.

Therefore request builder must include `stream_options: { include_usage: true }` for streaming usage capture.

### 4.2 OpenRouter

OpenRouter docs indicate:

1. Non-streaming responses include `usage`.
2. Streaming responses include usage in final chunk/event.
3. Usage fields include `prompt_tokens`, `completion_tokens`, `total_tokens`.

## 5. Architecture

### 5.1 Data contracts

1. Extend `ProviderEvent` with a usage variant carrying:
   - `prompt_tokens: u32`
   - `completion_tokens: u32`
   - `total_tokens: u32`
2. Extend `AgentEvent` with corresponding usage update event.
3. Keep usage payload simple and provider-agnostic at runtime boundary.

### 5.2 Provider layer

1. `openai_compat`:
   - add `usage` parsing to stream chunk type
   - emit `ProviderEvent::Usage` when final usage chunk arrives
   - add `stream_options.include_usage=true` in request payload
2. `openrouter`:
   - extract usage from final stream event (`Done` path or equivalent final payload)
   - emit `ProviderEvent::Usage`

### 5.3 Runtime relay

In `agent/runtime/turn.rs`, provider pass should capture latest usage event and forward it through `AgentEvent` to chat app.

### 5.4 Chat app state

`chat/app/events.rs` handles usage-update event and writes:

1. `context_usage.used_tokens = total_tokens`
2. `context_usage.max_tokens = resolved_max_tokens_for_current_model`

## 6. Max Tokens Resolution Strategy

### 6.1 Metadata-first

Extend model metadata to carry optional context length:

1. extend `ModelInfo` with `context_length: Option<u32>`
2. fill when provider model-list API exposes it

### 6.2 Fallback

If context length is unavailable:

1. fallback to `OpenAiConfig.max_tokens`
2. if both unavailable/invalid, keep `max_tokens=0` and UI shows `n/a`

### 6.3 Refresh points

Re-resolve current model max when:

1. app startup (`ChatApp::new`)
2. model switch (`set_model`)

## 7. Reliability and Error Handling

1. Missing usage in a turn does not fail the turn.
2. Malformed usage payload is ignored with logging; no panic.
3. Interrupted stream may not emit final usage; app keeps last valid display.
4. Usage event handling is side-effect safe (no timeline corruption, no turn-state mutation).

## 8. Testing Strategy

1. Provider unit tests:
   - OpenAI-compatible stream emits usage event when usage chunk appears
   - OpenRouter stream emits usage event from final event
2. Runtime tests:
   - `ProviderEvent::Usage` is relayed to `AgentEvent` usage update
3. Chat app tests:
   - usage update writes `context_usage`
   - max resolution uses context_length first, then config fallback
4. View tests:
   - mode row renders `used/max (%)` when max > 0
   - mode row renders `n/a` when max == 0

## 9. Acceptance Criteria

1. Real turns update context usage without test-only hooks.
2. OpenAI-compatible and OpenRouter both produce usage updates.
3. Max-token denominator follows metadata-first, config-fallback rule.
4. Existing model-row display remains stable and accurate.
5. No regressions in turn streaming, tool-call flow, or status rendering.

