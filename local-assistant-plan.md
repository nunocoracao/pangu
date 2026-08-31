# Pangu Local Assistant Redefinition

Status: Active execution plan  
Date: 2026-02-20

## Product definition

Build a local-first assistant similar to OpenClaw-style workflows, with:
- 100% local runtime
- minimal security friction
- fast onboarding
- durable memory without forced compaction
- remote interaction from phone (Telegram first)
- browser automation on user's behalf
- scheduling from anywhere
- live observability on laptop

## Target architecture

### 1) Core daemon (`pangu-core`)

Single local service that owns:
- task queue
- tool orchestration
- model routing
- memory writes/reads
- scheduling
- transport adapters

Implemented first as `/Users/nunocoracao/source/pangu/src/bin/pangu-core.rs`.

### 2) Clients and transports

- TUI/Desktop client: local monitor and control surface
- Telegram adapter: inbound/outbound messaging to daemon
- Future web/mobile client: same daemon API

### 3) Memory model

- Append-only memory log (events + artifacts)
- Retrieval index for relevance
- No destructive compaction required for correctness
- Optional summarization only as derived cache, never source of truth

### 4) Browser worker

Separate local worker process that executes browser tasks via Playwright/CDP:
- navigate
- click
- fill/submit
- extract content
- screenshots

Isolation requirement:
- browser failures/timeouts must not crash daemon

### 5) Scheduler

Local scheduler submits jobs into the same queue (chat/tool/browser tasks).

## Initial API contract (implemented)

`pangu-core` exposes:
- `GET /health`
- `POST /v1/tasks`
- `GET /v1/tasks`
- `GET /v1/tasks/{task_id}`
- `POST /v1/tasks/{task_id}/cancel`

Task kinds:
- `chat`
- `schedule`
- `browser`

Browser task schema includes:
- `navigate`
- `click`
- `fill`
- `wait_for`
- `extract_text`
- `screenshot`

## Recommended local model strategy

Default: Devstral Small 2 (24B class)  
A/B candidate: Qwen3-Coder-Next variant

Plan:
- keep backend pluggable
- track latency/quality by task type
- choose default per hardware profile

## Phased execution

1. Daemon baseline: queue + task API + status tracking.
2. Memory v1: append-only event store + retrieval endpoint.
3. Telegram adapter: map chats to task submissions + responses.
4. Browser worker v1: real Playwright/CDP execution.
5. Scheduler v1: recurring jobs + task injection.
6. Desktop/TUI monitor: show queue, task logs, memory events.
7. Onboarding wizard: one-command setup + transport/model checks.
8. Model A/B harness: Devstral Small 2 vs Qwen3-Coder variants.

## Current milestone done

- Added `pangu-core` binary with local task API and worker loop.
- Added explicit browser task primitives in daemon task schema.
