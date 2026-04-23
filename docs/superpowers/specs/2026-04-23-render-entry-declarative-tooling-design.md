# render_entry 声明式工具条目设计（阶段一）

## 1. 背景与目标

当前 `render_entry` 的工具相关渲染逻辑分散在分支中，状态、工具语义、结果展示规则耦合较高。  
本设计将工具条目改为“先声明、后渲染”的两阶段模型，优先覆盖 `ToolUse / ToolCall / Bash`，后续再扩展到所有 `TimelineEntry` 类型。

目标：

1. 工具条目渲染声明式化：状态、语义、结果展示规则集中建模。
2. 状态表达统一：运行/成功使用圆点样式，失败使用 `x`。
3. 结果默认紧凑：主时间线仅显示 1-2 行摘要，完整内容通过 `Ctrl+O` 展开。
4. 支持按“工具类型 × 内容类型”联合选择结果渲染模式。
5. 正确支持并行工具执行，避免状态和结果串线。

非目标（本阶段不做）：

1. 不重构全部 `EntryKind`（Message/Warning/Thinking/CodeBlock 留到后续阶段）。
2. 不引入跨条目聚合视图（保持逐条目展开）。

## 2. 架构概览

采用两阶段流程：

1. **声明层（Descriptor）**：从 `TimelineEntry` 生成 `EntryDescriptor`（纯数据，不做 UI 拼装）。
2. **渲染层（Renderer）**：`render_entry` 仅消费 `EntryDescriptor` 产出最终 `Line` 列表。

核心约束：

1. `render_entry` 不再散落状态/语义判断分支。
2. 状态、语义、结果模式在声明层统一决策。
3. 详情展开继续沿用现有 `Ctrl+O` 行为。

## 3. 数据模型（声明层）

建议引入（名称可微调）：

- `EntryDescriptor`
  - `status_visual`: `RunningDot | EndedDot | ErrorX`
  - `tool_semantic`: 工具名 + 智能语义摘要（参数/目标）
  - `result_preview`: 紧凑摘要（1-2 行）
  - `result_full`: 完整结果（供 `Ctrl+O`）
  - `render_mode`: 由工具类型与内容类型联合判定
  - `call_identity`: 并行场景下的条目身份（优先 `call_id`）

- `RenderMode`
  - `Markdown`
  - `Code { language: Option<String> }`
  - `Diff`
  - `Plain`

## 4. 组件拆分

仅针对工具条目先拆分：

1. `src/chat/view/timeline/render_entry/tooling/descriptor.rs`
   - `build_tool_descriptor(entry: &TimelineEntry) -> EntryDescriptor`
   - 处理状态映射、语义提取、模式判定、摘要生成
2. `src/chat/view/timeline/render_entry/tooling/renderer.rs`
   - `render_tool_descriptor(descriptor, theme, width) -> Vec<Line>`
   - 负责头部与结果区排版
3. `src/chat/view/timeline/render_entry/tooling.rs`
   - 保留编排入口，桥接 descriptor 与 renderer

## 5. 渲染规则

### 5.1 头部结构

首行固定：

- 状态标记（运行/成功圆点，失败 `x`）
- 工具名
- 工具语义摘要（target/参数关键字段）

### 5.2 结果展示

默认主时间线：

- 仅显示 1-2 行摘要（紧凑模式）

`Ctrl+O` 展开：

- 展示完整结果内容（`result_full`）

模式细则：

1. `Diff`：摘要显示关键 hunk/文件级提示；完整 diff 在展开区。
2. `Markdown`：摘要为 1-2 行文本要点；展开走 markdown 渲染。
3. `Code`：摘要显示语言+首行；展开保留代码块样式。
4. `Plain`：摘要按宽度截断 1-2 行；展开显示全文。

## 6. 模式判定策略（工具类型 × 内容类型）

先按工具类型分流（如 `bash` / `edit_file` / `read_file` / `agent`），再结合内容类型判定最终 `RenderMode`。  
未命中规则时回退 `Plain`，但保留状态与语义头部。

## 7. 并行工具执行语义

1. 每个工具调用以 `call_id` 为主身份键，独立生成 descriptor。
2. 允许多个运行中条目并存。
3. 结束事件仅影响对应 `call_id` 条目。
4. 同名工具并行也必须可区分（依靠语义摘要与身份键）。
5. 展开行为按条目粒度，不做跨工具合并。

## 8. 错误处理

1. 参数 JSON 解析失败时不静默：摘要给出安全回退文案（如 `invalid args`）。
2. 原始详情保留到展开视图，便于排错。
3. 未识别工具或内容类型使用默认模式，不中断渲染。

## 9. 测试计划（增量）

在现有 `render_entry` 单测基础上扩展：

1. 状态映射：运行/成功圆点，失败 `x`。
2. 模式分发：工具类型 × 内容类型组合覆盖。
3. 摘要约束：默认最多 1-2 行。
4. 展开行为：`Ctrl+O` 后完整 markdown/code/diff 可见。
5. 并行安全：同名工具并行与多调用并行不串线。

## 10. 后续阶段（阶段二预留）

在阶段一稳定后，将 `Message/Warning/Thinking/CodeBlock` 逐步接入统一 `EntryDescriptor` 模型，完成 `render_entry` 全量声明式化。
