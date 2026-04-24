# Tool Arguments Error Coverage Design

## Problem Statement

Blazar currently handles malformed tool-argument JSON with a mixed strategy:

1. Generic parsing remains strict with minimal safe repair (`extract_json_payload`, control-character escaping).
2. `bash` has an extra targeted fallback for unescaped inner quotes.

This already fixes the most frequent `bash` case, but coverage is still incomplete for other high-frequency failures:

1. Truncated/partial JSON from streaming boundaries.
2. Leading/trailing wrapper junk around otherwise valid JSON.
3. Type/shape drift after parse succeeds but tool input is semantically unusable.
4. Repeated malformed retries with no progressive guidance loop.

Required outcome: a design that covers most real-world failures while preserving Blazar’s safety rule of **strict by default, targeted recovery only when confidence is high**.

## Goals

1. Increase successful recovery for common malformed tool arguments without introducing unsafe silent execution.
2. Keep non-`bash` behavior strict unless a repair is explicitly approved by policy.
3. Make every repair observable and explainable (logs + model feedback).
4. Keep runtime ownership in Blazar runtime modules, not UI/rendering helpers.
5. Provide a testable error taxonomy and coverage matrix.

## Non-Goals

1. Building a fully permissive “fix any JSON” engine.
2. Hiding malformed input from the model with silent fallbacks.
3. Broad schema coercion that changes tool intent.
4. Rewriting provider streaming protocol.

## Approaches

### Approach A: Strict Fail-Fast Everywhere

- Parse once; on failure return actionable error and force model retry.
- No repair except current extraction/control-char handling.

Trade-off:

- Safest semantics.
- Lowest success rate for known recoverable malformed `bash` patterns.

### Approach B: Global Permissive Auto-Repair

- Apply broad heuristic repair to all tools and execute if parse succeeds.

Trade-off:

- Highest apparent short-term success.
- Highest risk of wrong execution, hard-to-debug side effects, and hidden model quality regressions.

### Approach C (Recommended): Layered Recovery Policy Engine

- Keep strict parser as baseline.
- Add bounded repair stages with explicit confidence rules.
- Separate generic low-risk repairs from tool-profile repairs (starting with `bash`).
- Emit typed failure reasons and repair evidence.

Trade-off:

- Slightly more runtime complexity.
- Best balance of safety, recoverability, and debuggability.

## Chosen Approach

Adopt **Approach C** with staged rollout:

1. Phase 1: formalize error taxonomy + strengthen `bash` profile + observability.
2. Phase 2: add opt-in profiles for other tools with explicit policy guards.
3. Phase 3: corpus-driven tuning using real malformed samples from logs.

## Design

### 1. Error Taxonomy

Classify parse/validation failures into stable categories:

1. `invalid_json_unescaped_quotes`
2. `invalid_json_control_chars`
3. `invalid_json_truncated_payload`
4. `invalid_json_wrapped_payload`
5. `invalid_json_multiple_payloads`
6. `invalid_shape_non_object_root`
7. `invalid_shape_schema_mismatch`

Each category must map to one of:

1. `repaired_and_executed`
2. `rejected_with_guidance`
3. `rejected_after_repair_budget`

### 2. Recovery Pipeline

Introduce a deterministic parse pipeline in runtime scheduling:

1. **Normalize**: strip known wrappers/tags and extract first valid top-level payload.
2. **Strict parse**: `serde_json::from_str`.
3. **Generic safe repair** (policy: `global_safe`):
   - control chars in string values
   - wrapper extraction when object/array boundaries are unambiguous
4. **Tool profile repair** (policy: tool-specific):
   - `bash`: unescaped inner quotes in command-like string fields, bounded truncated-payload completion
5. **Shape validation**:
   - object root required unless tool profile explicitly allows otherwise
   - reject on semantic mismatch with typed guidance

If any stage succeeds, produce canonical arguments and execute. If not, return typed error guidance.

### 3. Repair Budget and Confidence Gates

Add strict safety bounds:

1. `max_repair_passes = 2` (generic + tool profile).
2. Do not chain more than one structural mutation and one escaping mutation.
3. For truncated payload completion, allow only bracket/brace closure (no content synthesis).
4. Abort repair if mutation crosses configured delta size threshold (e.g., >15% payload growth).

This prevents “inventing” arguments while still recovering incomplete stream boundaries.

### 4. Tool Policy Profiles

Define policy per tool name:

1. `strict_only`: parse strict + generic safe; no tool-specific repair.
2. `strict_plus_profile`: parse strict + generic safe + tool profile repair.
3. `deny_repair`: strict parse only (for highest-risk tools if needed later).

Initial policy:

1. `bash` -> `strict_plus_profile`
2. all other tools -> `strict_only`

### 5. Guidance Loop

When rejected, return actionable error payload with:

1. normalized category code
2. short fix recipe (escape quotes, encode newlines, ensure JSON object root)
3. previous-attempt hint if same malformed signature repeats

This keeps model retries focused and reduces repeated malformed calls.

### 6. Observability Contract

Emit structured runtime events:

1. `tool_args_parse_failed` with `tool_name`, `error_category`, `raw_preview`
2. `tool_args_repaired` with `repair_stage`, `repair_kind`, `bytes_changed`
3. `tool_args_repair_rejected` with `reason`, `budget_state`

These events become the input corpus for future policy tuning.

### 7. Test Coverage Matrix

Add/extend runtime tests for each category:

1. `bash` unescaped quotes repaired and executed.
2. non-`bash` unescaped quotes rejected.
3. control chars repaired.
4. wrapped payload extracted and parsed.
5. truncated payload closure repaired for `bash` only when closure-only completion is sufficient.
6. multiple payload ambiguity rejected.
7. object-root/type mismatch rejected with typed guidance.
8. repeated malformed signature escalates guidance.

Also add corpus tests from anonymized log samples to prevent regressions.

## Rollout Plan

1. Introduce taxonomy enums + repair outcome struct (no behavior change).
2. Wire scheduler to emit category-aware parse outcomes.
3. Add bounded truncated-payload repair for `bash`.
4. Add observability events and signature-based repeat escalation.
5. Expand test matrix and log-sample corpus tests.

## Risks and Mitigations

1. **Risk**: Over-repair executes wrong command.
   - **Mitigation**: profile-gated repair + confidence gates + low mutation budget.
2. **Risk**: Repair logic becomes unmaintainable.
   - **Mitigation**: small composable repair functions with category-specific tests.
3. **Risk**: Silent masking of model issues.
   - **Mitigation**: always emit repair telemetry and include repaired flag in runtime path.

## Review Checklist (Self-Validated)

1. No placeholders or TBDs.
2. No contradiction with current strict-default safety direction.
3. Scope is one implementation track (runtime parse/repair path), not a platform rewrite.
4. Ambiguous behaviors (truncated repair boundaries, policy scope) are explicit.
