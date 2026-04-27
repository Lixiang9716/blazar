# Timeline Streaming Color Design

**Date:** 2026-04-27  
**Scope:** Chat timeline streaming/thinking visual emphasis

## Goal

Give streaming output a clearer visual signal by adding moderate color emphasis to timeline thinking content, without changing layout or product state.

## Decision Summary

Chosen approach: **A** (timeline-only, thinking-focused, moderate intensity).

- Apply emphasis only to `EntryKind::Thinking` in timeline rendering.
- Keep marker behavior and text structure unchanged.
- Do not change users status row or other entry kinds.

## Architecture and Boundaries

This is a **render-layer-only** change.

- No new state fields in `ChatApp`.
- No protocol/runtime event changes.
- No data flow changes between provider/runtime/app.

Likely touchpoints:

- `src/chat/view/timeline/render_entry/status.rs` (thinking entry text style)
- `src/chat/view/timeline/render_entry.rs` (if routing changes are needed)
- `tests/unit/chat/view/timeline/tests.rs` and/or render-entry tests

## Rendering Behavior

For `EntryKind::Thinking`:

1. Keep existing marker style (`theme.marker_thinking`).
2. Render thinking body with a more prominent style than regular body text, but still in the same visual family (no background block, no extra decorations beyond current style policy).
3. Preserve existing wrapping, spacing, and detail-toggle behavior.

## Error Handling / Safety

- No new failure surfaces are introduced.
- If style mapping is unavailable, fallback remains existing timeline/body styles.
- Avoid broad refactors to prevent regressions in non-thinking entries.

## Testing Strategy

Add/adjust unit tests to verify:

1. Thinking entry still renders in timeline flow.
2. Thinking entry style differs from regular body style in rendered spans (moderate highlight present).
3. No behavior regressions for banner/message/tool entries in existing timeline tests.

## Non-Goals

- No users-panel color changes.
- No dynamic “only while streaming” state-coupled coloring.
- No new theme token unless existing theme contract proves insufficient.
