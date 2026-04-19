# Graph Report - .  (2026-04-18)

## Corpus Check
- Corpus is ~6,969 words - fits in a single context window. You may not need a graph.

## Summary
- 114 nodes · 163 edges · 11 communities detected
- Extraction: 71% EXTRACTED · 29% INFERRED · 0% AMBIGUOUS · INFERRED: 48 edges (avg confidence: 0.62)
- Token cost: 0 input · 0 output

## God Nodes (most connected - your core abstractions)
1. `ChatApp` - 11 edges
2. `render_mascot_lines()` - 7 edges
3. `slime_idle_animation()` - 7 edges
4. `SpriteAnimation` - 6 edges
5. `animation_with_frame_time()` - 6 edges
6. `render_mascot_plain()` - 6 edges
7. `slime_idle_config()` - 6 edges
8. `build_frame()` - 5 edges
9. `pixel_pair_to_cell()` - 5 edges
10. `render_mascot()` - 5 edges

## Surprising Connections (you probably didn't know these)
- `TerminalFrame` --semantically_similar_to--> `ratatui-image`  [INFERRED] [semantically similar]
  blazar-context/src/welcome/sprite.rs → awesome-ratatui/README.md
- `Composer TextArea` --semantically_similar_to--> `ratatui-textarea`  [INFERRED] [semantically similar]
  blazar-context/src/chat/app.rs → awesome-ratatui/README.md
- `render_to_lines_for_test` --calls--> `render_mascot_plain()`  [EXTRACTED]
  blazar-context/src/chat/view.rs → blazar-context/src/welcome/mascot.rs
- `render_mascot()` --shares_data_with--> `TerminalFrame`  [INFERRED]
  blazar-context/src/welcome/mascot.rs → blazar-context/src/welcome/sprite.rs
- `render_mascot_plain()` --shares_data_with--> `TerminalFrame`  [INFERRED]
  blazar-context/src/welcome/mascot.rs → blazar-context/src/welcome/sprite.rs

## Hyperedges (group relationships)
- **Chat Event Loop Pipeline** — chat_app_run_terminal_chat, chat_input_from_key_event, chat_app_handle_action, chat_view_render_frame [INFERRED 0.89]
- **Mascot Rendering Pipeline** — mascot_slime_idle_animation, sprite_from_png_bytes, sprite_build_frame, sprite_pixel_pair_to_cell, mascot_render_mascot_lines [INFERRED 0.87]

## Communities

### Community 0 - "Sprite Rendering"
Cohesion: 0.14
Nodes (15): animation_with_frame_time(), build_frame(), high_fps_uses_sub_millisecond_frame_time(), pixel_pair_to_cell(), Rgb, same_color_pair_becomes_full_block(), SpriteAnimation, SpriteError (+7 more)

### Community 1 - "Chat Runtime"
Cohesion: 0.15
Nodes (19): ChatApp, Composer TextArea, ChatApp::handle_action, run_terminal_chat, ChatApp::send_message, ChatApp::submit_composer, InputAction::from_key_event, InputAction (+11 more)

### Community 2 - "ChatApp API"
Cohesion: 0.17
Nodes (4): ChatApp, run_terminal_chat(), Author, ChatMessage

### Community 3 - "Mascot Integration"
Cohesion: 0.32
Nodes (12): Styled mascot rendering preserves color without raw ANSI, render_spirit_pane, MascotConfig, render_mascot(), render_mascot_lines(), render_mascot_plain(), slime_idle_animation(), slime_idle_config() (+4 more)

### Community 4 - "View Rendering Tests"
Cohesion: 0.23
Nodes (5): render_chat_pane(), render_composer(), render_frame(), render_messages(), render_spirit_pane()

### Community 5 - "Ratatui Ecosystem Curation"
Cohesion: 0.17
Nodes (12): Awesome Main Contributing Guide, Consistent categorization and concise descriptions, Awesome List Entry Format, Pull Request Guidelines, Apps, Awesome Ratatui, Development Tools, Libraries (+4 more)

### Community 6 - "Runtime Input Tests"
Cohesion: 0.33
Nodes (0): 

### Community 7 - "Theme Tokens"
Cohesion: 0.67
Nodes (1): ChatTheme

### Community 8 - "Input Mapping"
Cohesion: 0.67
Nodes (1): InputAction

### Community 9 - "License Terms"
Cohesion: 1.0
Nodes (2): MIT License, Warranty Disclaimer

### Community 10 - "Module Surface"
Cohesion: 1.0
Nodes (0): 

## Knowledge Gaps
- **14 isolated node(s):** `TerminalCell`, `Author`, `ChatMessage`, `ChatTheme`, `ChatTheme` (+9 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `License Terms`** (2 nodes): `MIT License`, `Warranty Disclaimer`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Module Surface`** (1 nodes): `mod.rs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `TerminalFrame` connect `Mascot Integration` to `Sprite Rendering`, `Ratatui Ecosystem Curation`?**
  _High betweenness centrality (0.256) - this node is a cross-community bridge._
- **Why does `build_frame()` connect `Sprite Rendering` to `Mascot Integration`?**
  _High betweenness centrality (0.235) - this node is a cross-community bridge._
- **Why does `Widgets` connect `Ratatui Ecosystem Curation` to `Chat Runtime`?**
  _High betweenness centrality (0.130) - this node is a cross-community bridge._
- **Are the 5 inferred relationships involving `animation_with_frame_time()` (e.g. with `.from_png_bytes()` and `tick_does_not_advance_before_one_interval()`) actually correct?**
  _`animation_with_frame_time()` has 5 INFERRED edges - model-reasoned connections that need verification._
- **What connects `TerminalCell`, `Author`, `ChatMessage` to the rest of the system?**
  _14 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Sprite Rendering` be split into smaller, more focused modules?**
  _Cohesion score 0.14 - nodes in this community are weakly interconnected._