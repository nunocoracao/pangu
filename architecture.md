# Pangu Architecture Plan (Claude Code-like Experience)

Status: Draft for review before implementation
Owner: Pangu project
Scope: Product and technical architecture decisions for phased rebuild

## 1. Objectives

Primary objective:
- Evolve Pangu from a local chat+tools prototype into a reliable local coding agent workflow that feels closer to Claude Code in terminal ergonomics, safety, and extensibility.

Constraints:
- Keep existing download and loading screens (explicitly preserved).
- Defer implementation until this plan is reviewed and agreed.
- Favor deterministic, testable behavior over "magic" autonomy.

## 2. Current State (as of this document)

High-level observations from current codebase:
- Main runtime is centralized in `src/main.rs` with orchestration, events, generation flow, RAG, tool execution, and permission handling interleaved.
- TEA-ish app structure exists (`src/app.rs`, `src/update.rs`, `src/tui/ui.rs`) but side effects are still mostly outside a strict reducer/effects boundary.
- Tool framework is in place (`src/tools/*`), including parser support for multiple tool-call formats.
- Permission model exists conceptually (`src/tools/permission.rs`), but runtime wiring is partial (`TODO` for persistent always-allow in generation flow).
- RAG/session persistence already exists and should be preserved as optional memory features.

Implication:
- We should prioritize architectural separation before adding capabilities.

## 3. Product Direction: Claude Code-like UX (Local-first)

Target UX characteristics:
- Fast keyboard-first terminal interaction.
- Clear distinction between assistant reasoning/output and executed actions.
- Explicit tool and permission surfaces with minimal ambiguity.
- Trust-first safety model (deny by default for dangerous operations).
- Reproducible runs through logs/traces/event history.

Non-goals (for now):
- Full cloud parity with hosted coding agents.
- Invisible background automation without explicit user control.

## 4. Guiding Architecture Decisions

### A. Runtime split: Core vs Adapters
Decision:
- Create strict layers:
  - `core`: domain state machine (messages, actions, permissions, plans, tasks).
  - `runtime`: effect executor (model calls, tools, fs, network, subprocess).
  - `ui`: rendering and input mapping only.

Why:
- Keeps behavior testable and removes logic from rendering/event glue.

### B. Event-sourced state transitions
Decision:
- Move toward explicit event log + reducer transitions for all user/model/tool actions.

Why:
- Easier debugging, replay, and deterministic tests.

### C. Capability registry and policy engine
Decision:
- Model all tools/MCP/skills as capabilities with metadata:
  - id, risk level, side-effect class, required permission scope.

Why:
- Uniform permission and audit handling across phases.

### D. Safety by construction
Decision:
- Introduce hard policy gate between model intent and effect execution.
- Never let model output directly execute commands without policy+user approval path.

Why:
- Required before destructive phases (bash/write/edit/delete etc.).

### E. Config profile system
Decision:
- Add profile-based config (`safe`, `balanced`, `power-user`) to control autonomy.

Why:
- Different users need different risk/performance defaults.

## 5. Proposed Phase Plan

## Phase 0 - Foundation cleanup + base chat UX (no tools)

Goal:
- Stabilize architecture and deliver a polished core chat interface.

Includes:
- Refactor `main.rs` into smaller runtime modules:
  - session bootstrap
  - model lifecycle
  - event loop
  - generation pipeline
- Remove tool execution path from active flow in this phase.
- Redesign TUI chat experience (while preserving current download/loading screens).
- Add structured telemetry events (local file logs) for major actions.

Acceptance criteria:
- No tool execution reachable.
- Chat streaming stable with cancel/retry.
- Cleaner module boundaries and integration tests for core state transitions.
- `cargo build` warning count reduced to zero (or explicitly allowlisted with rationale).

### Phase 0 execution plan (detailed)

Step 0.1 - Freeze scope and feature flags
- Add a `phase0_chat_only` runtime mode that disables tool parsing/execution paths.
- Keep download/loading screens untouched.

Step 0.2 - Runtime decomposition
- Split `src/main.rs` into focused runtime modules:
  - bootstrap/startup
  - model lifecycle
  - event orchestration
  - generation service
- Keep behavior identical while moving code.

Step 0.3 - Chat UX redesign (Claude Code-like baseline)
- Define one clear transcript surface and one action/status surface.
- Improve keyboard navigation, cancel, retry, and scroll behavior.
- Keep rendering logic side-effect free.

Step 0.4 - Warning burndown (`cargo build`)
- Remove dead code that belongs to later phases, or gate it behind feature flags.
- If a symbol is intentionally deferred, annotate with explicit phase comment and lint allow at the narrowest scope.
- Target: zero warnings in default build.

Step 0.5 - Test and observability baseline
- Add reducer-level tests for key state transitions.
- Add integration test for startup -> first prompt -> stream -> cancel -> new prompt.
- Add structured event logs with per-turn correlation id.

Step 0.6 - Exit criteria review
- Confirm phase goals + warning target + tests + UX checklist are all met before Phase 1.

### Current warnings baseline (from `cargo build`)

Observed on current tree: 17 warnings.

Category A - Unused app state / methods (`src/app.rs`)
- `PendingToolCall.is_write` unused.
- `App.pending_tool_calls` unused.
- Permission-selection helper methods unused.

Category B - Tool framework API not fully consumed yet (`src/tools/mod.rs`, `src/tools/parser.rs`)
- Unused trait methods/docs helpers (`description`, `parameters`, `generate_docs`).
- Unused registry helpers (`tool_names`, `generate_all_docs`).
- Unused fields/methods (`ToolResult.success`, `ToolResult.truncated`, `ToolParams.contains`, `ToolCall.raw`, parser `contains_tool_call`).

Category C - Permission manager not wired end-to-end (`src/tools/permission.rs`)
- `PermissionCheckResult` and most `PermissionManager` methods/fields unused in runtime.

Category D - Minor lifecycle helper unused (`src/session.rs`)
- `SessionManager::session_path` unused.

Phase 0 handling policy:
- If functionality is postponed to later phases, isolate behind features and remove unused symbols from default build path.
- Do not keep broad `#[allow(dead_code)]` as a permanent workaround in core modules.

## Phase 1 - Read-only tools

Goal:
- Enable repository inspection safely.

Includes:
- Read-only capability set only (`list_files`, `read_file`, `grep`, maybe `fetch` read-only web).
- Unified tool result rendering in UI (separate from assistant prose).
- Tool timeout, output truncation, and deterministic formatting.

Acceptance criteria:
- No write/exec side effects available.
- Read tools visible, auditable, and interruptible.

## Phase 2 - Permission system (full wiring)

Goal:
- Make permission model first-class and complete.

Includes:
- Runtime integration of `PermissionManager` with persistent allow/deny policies.
- Granular scopes: per-tool, per-path, per-session/permanent.
- Explicit permission prompts with clear risk labels.
- "Preview before apply" for any planned write operation.

Acceptance criteria:
- No side-effect tool can run without a policy decision.
- Persisted permissions work end-to-end.

## Phase 3 - Project instruction files (`AGENTS.md` / `CLAUDE.md`)

Goal:
- Respect repository-level operating instructions safely and predictably.

Includes:
- Deterministic loading precedence rules (e.g., root + nearest parent + local overrides).
- Context budget management for instruction injection.
- Validation and conflict reporting in UI.

Acceptance criteria:
- Instruction resolution is transparent and inspectable.

## Phase 4 - Skills

Goal:
- Introduce composable task workflows/extensions.

Includes:
- Skill discovery/indexing.
- Skill execution contract (inputs/outputs/permissions).
- Skill sandbox constraints and audit trail.

Acceptance criteria:
- Skills cannot bypass tool permission policy.

## Phase 5 - MCP support (local, remote, OAuth)

Goal:
- Connect external capability providers safely.

Includes:
- MCP client abstraction + transport adapters.
- Auth flows (including OAuth) and secret storage strategy.
- Per-server trust level + per-method permission mediation.

Acceptance criteria:
- MCP capabilities appear as typed capabilities under the same policy engine.
- Remote servers cannot silently escalate privileges.

## Phase 6 - Bash + CLI discovery/execution

Goal:
- Controlled shell execution for real coding workflows.

Includes:
- Command planning + dry-run preview.
- Allow/deny based on command policy (blocklist + allowlist + risk classifier).
- Sandboxed cwd/env defaults.
- Resource limits: timeout, output cap, concurrent process cap.

Acceptance criteria:
- All exec actions are user-approved or covered by explicit trusted policy.
- Full command audit log with exit codes and diffs for filesystem changes.

## Phase 7 - Code generation and file mutation workflows

Goal:
- End-to-end task completion with proposed and applied code changes.

Includes:
- Plan -> patch -> verify loop.
- Diff-first UX, multi-file change sets, revert support.
- Verification hooks (tests/lint/build) before final apply when configured.

Acceptance criteria:
- No direct blind writes; edits are patch-based and reviewable.
- Rollback path always available.

## 6. Destructive Phase Warning and Gating

Requested warning policy:
- Treat phases as "destructive-risk" starting here:
  - Phase 6: shell execution can damage system/data.
  - Phase 7: code/file mutation at scale can cause project/system damage.

Gate before entering Phase 6:
- Mandatory safety review checkpoint document approved.
- Policy engine complete and enforced.
- Default mode = safe deny for destructive commands.
- Optional extra isolation agreed (recommended):
  - sandbox/jail strategy (e.g., containerized workspace execution),
  - snapshot/backup strategy,
  - high-risk command guardrails.

If these are not complete, do not proceed past Phase 5.

## 7. Cross-cutting Requirements

Security:
- Path traversal protection, shell injection hardening, remote content trust boundaries.

Reliability:
- Cancellable operations, bounded retries, and clear failure states.

Performance:
- Keep UI responsive during model/tool operations.
- Prefer streaming and incremental rendering.

Observability:
- Action/event logs with correlation IDs for each user turn.

Testing:
- Unit tests for reducer/policy.
- Integration tests for runtime pipelines.
- End-to-end golden transcript tests for key scenarios.

## 8. Proposed Codebase Restructure (incremental)

Target top-level module intent:
- `src/core/` - state, actions, events, policies, capabilities.
- `src/runtime/` - executors for model/tools/mcp/shell.
- `src/ui/` - TUI widgets + input mapping.
- `src/platform/` - filesystem/process/network adapters.

Migration approach:
- Do not big-bang rewrite.
- Move one concern at a time with compatibility shims.

## 9. Decision Log (initial)

Accepted:
- Preserve current download/loading screens.
- Phase-based delivery with safety gates.
- Architecture-first before implementation.

Open decisions (must settle before coding):
1. Exact TUI interaction model to emulate Claude Code:
   - single-pane vs split activity pane,
   - how to display tool calls and permission prompts.
2. Policy defaults:
   - deny-all destructive by default vs ask-once default.
3. Sandboxing strategy for Phase 6+:
   - native process restrictions only vs containerized execution.
4. MCP auth secret storage:
   - OS keychain integration vs encrypted file with passphrase.
5. Memory behavior:
   - default on/off for RAG and retention limits.

## 10. Immediate Next Step (before implementation)

Do a design review of this document and lock:
- UX wireframe for Phase 0.
- Capability policy schema.
- Safety gate checklist for entering Phase 6.

Only after these are approved should coding begin.
