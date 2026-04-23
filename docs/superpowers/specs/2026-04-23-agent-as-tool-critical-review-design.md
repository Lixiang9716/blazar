# Agent-as-Tool Critical Review (Value-Chain Lens)

## Scope

This document is a critical review of the current Agent-as-Tool design (Capability Kernel direction), focused on architectural decision quality rather than feature implementation details.

Review lens: **user controllability -> runtime correctness -> evolution cost**.

---

## 1. Evaluation Framework

We evaluate the design with three hard criteria:

1. **Explainability**  
   Can users and developers clearly understand what is invoked, what failed, and why?

2. **Consistency**  
   Under parallelism, retries, and cancellation, does the system preserve deterministic behavior and semantics?

3. **Evolution Friction**  
   When adding tools/protocols/rules, is change localized and regression risk bounded?

This avoids architecture-by-style and keeps judgement tied to user value and operating cost.

---

## 2. Strengths (What This Design Truly Solves)

1. **Capability abstraction decouples orchestration from protocol details**  
   Runtime-level orchestration no longer directly owns ACP SDK concerns, reducing protocol-upgrade blast radius.

2. **Concurrency policy is explicit and testable**  
   `CapabilityClaim + ConflictPolicy` turns race-prone behavior into contract-like semantics.

3. **Migration continuity is preserved**  
   Keeping Tool facade as a migration shell avoids big-bang rewrite risk during structural changes.

4. **Identity visibility is improving**  
   ACP/local kind distinctions in timeline are the right direction for user trust and operator debugging.

**Strength preconditions:** claim normalization, status/error mapping consistency, and user-visible diagnostics must stay aligned.

---

## 3. Weaknesses and Structural Costs (Critical Findings)

1. **Layer count increases cognitive and debugging load**  
   Tool/Capability/Adapter/Runtime boundaries are better, but troubleshooting path is longer without strict contracts.

2. **Error semantics can drift across layers**  
   Provider errors, protocol terminal statuses, and execution errors may diverge unless normalized in one taxonomy.

3. **Scheduler correctness has combinatorial test pressure**  
   As claim/access/resource varieties grow, regression probability rises unless a stable matrix-based contract suite is enforced.

4. **Facade permanence risks two-track architecture**  
   If compatibility behavior is never retired, Capability Kernel cannot become the single source of truth.

5. **“ACP-first” may still leak into abstraction shape**  
   Runtime is less SDK-coupled now, but capability semantics are still strongly molded by ACP usage patterns.

---

## 4. Design Recommendations

### 4.1 Canonical error taxonomy (mandatory)

Define one cross-layer error model (`provider/protocol/capability/execution`) with mandatory mapping fields:

- retryability
- user-facing severity
- operator log severity
- structured error code

### 4.2 Scheduler contract suite (mandatory)

Lock behavior with a compact but fixed matrix for:

- claim normalization
- conflict grouping
- batch ordering
- replay ordering
- cancellation boundaries

### 4.3 Facade exit strategy (time-boxed)

Document which Tool facade semantics are:

- temporary compatibility
- permanent product API

and add retirement criteria for temporary paths.

### 4.4 Capability-level observability

Attach stable correlation data to execution and timeline paths:

- capability handle
- normalized claims
- batch id
- replay order

to make parallel failures diagnosable without inference.

### 4.5 Second-protocol readiness probe

Before introducing another real protocol, add a synthetic adapter conformance test to verify that kernel contracts are not ACP-specialized in practice.

---

## 5. Decision

The current direction is **strategically correct** and worth continuing.

The biggest risk is not missing functionality; it is **governance debt** (contract drift, semantic drift, and observability gaps) as complexity grows.

Next phase should prioritize:

1. contract hardening,
2. semantic unification,
3. observability completeness,

before major new capability expansion.

---

## 6. Non-Goals for This Review

- Replacing the current architecture direction.
- Immediate implementation planning details.
- UI redesign beyond observability needs.

---

## 7. Implementation Status (2026-04-23)

- ✅ Task 4 complete: Tool facade governance now includes compatibility-tier metadata.
- Added `ToolCompatibilityTier` with `KernelNative` and `CompatibilityBridge`.
- `Tool` trait now defaults to `KernelNative` via `compatibility_tier()`.
- `ToolRegistry` now exposes `compatibility_tier(name)` for governance introspection.
- `bash` is explicitly tagged `CompatibilityBridge` under current architecture.
- Follow-up RED→GREEN evidence: policy tiers now override per-tool defaults for governed bridges (including `vet`) so registry-level metadata stays visible.
