# Timeline Markdown Body + Structured Metadata Design

## 1. Problem

Timeline entries currently mix multiple rendering styles. Some assistant message content already uses Markdown, while many other entry bodies/details remain plain text. This creates inconsistent readability for code blocks and diff content.

We need to make **entry body/details fields Markdown-capable** while keeping **status/metadata fields structured** and stable.

## 2. Goals and Scope

### Goals

1. Render timeline正文类字段 with a unified Markdown pipeline.
2. Keep状态/元数据字段结构化渲染 (tool status, ids, counters, badges, markers).
3. Keep `Actor::User + EntryKind::Message` as plain text (no Markdown behavior change).
4. Support fenced code blocks and ` ```diff ` highlighting in body and details.

### In Scope

- Entry body/details rendering for:
  - `Message` (assistant/system side only)
  - `Thinking`
  - `CodeBlock`
  - `Warning`
  - `Hint`
  - `ToolUse`
  - `ToolCall`
  - `Bash`
- `details` expansion (Ctrl+O) rendering path.

### Out of Scope

- Changing timeline state ownership (`ChatApp` remains source of truth).
- Replacing structured metadata headers with Markdown.
- Changing user-message rendering to Markdown.

## 3. Design

### 3.1 Responsibility Split

Use a strict split:

1. **Structured header renderer (per-entry)**
   - Keep existing header/status/metadata output shape and style.
2. **Shared Markdown body renderer**
   - Render `body` and `details` text content via one helper pipeline.

This keeps semantic status rendering explicit while unifying text readability.

### 3.2 Shared Markdown Rendering Helper

Introduce shared helper(s) under timeline render-entry area, e.g.:

- `render_markdown_body(...)`
- `render_markdown_details(...)`

Expected inputs:

- raw text
- width
- theme
- first-line prefix and continuation prefix

Expected behavior:

- preserve existing marker/indent strategy from caller
- apply paragraph normalization and fenced-code segmentation
- support diff fences via existing Markdown/theme path

### 3.3 Per-Entry Integration

Each entry renderer keeps its structured header logic and delegates正文 text rendering:

- Header/metadata: unchanged and entry-specific.
- Body/details: delegated to shared Markdown helper.

User messages remain a special case:

- `Actor::User + EntryKind::Message` continues to use existing plain-text wrapped rendering.

## 4. Data Flow

1. Timeline chooses renderer by `EntryKind` as today.
2. Renderer emits structured header spans.
3. Renderer passes `body` to shared Markdown body helper (except user messages).
4. Timeline details-expansion path delegates `details` text to shared Markdown details helper.
5. Final lines remain in same frame flow and scroll accounting.

## 5. Error Handling and Performance

1. If Markdown parsing/render path fails for an entry text block, fallback to plain-text rendering for that text block only.
2. Keep structured header rendering independent from Markdown success/failure.
3. Keep current output-capping strategy where already present (e.g., bash output limits).
4. Execute details Markdown rendering only when details are expanded.

## 6. Testing Strategy

1. Extend/adjust `chat_render` tests for Markdown body coverage on:
   - ToolUse, ToolCall, Bash, Warning, Hint, Thinking.
2. Add/update details-expansion tests to verify Markdown/diff rendering in `details`.
3. Add/keep regression test ensuring `Actor::User + Message` stays plain text.
4. Snapshot updates only when output semantics intentionally changed.

## 7. Acceptance Criteria

1. Body/details fields of in-scope entry kinds render via Markdown helper path.
2. Status/metadata fields stay structured and visually stable.
3. User messages remain plain-text rendering behavior.
4. Diff code fences render through Markdown path.
5. Quality gates remain green.

## 8. Risks and Mitigations

1. **Risk:** visual drift in status lines.
   - **Mitigation:** keep status/header rendering isolated from Markdown helper.
2. **Risk:** performance regression for long details.
   - **Mitigation:** render details Markdown only when expanded.
3. **Risk:** inconsistent behavior across entry kinds.
   - **Mitigation:** single shared Markdown helper and targeted regression tests.
