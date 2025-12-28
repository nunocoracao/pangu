# Pangu

Terminal-based agentic coding assistant with local LLM inference.

## Quick Start

```bash
# Build with Metal GPU acceleration (macOS)
cargo build --release --features metal

# Download the model
./models/download.sh

# Run
cargo run --release --features metal
```

## Architecture

- **TEA (The Elm Architecture)** - Model/Update/View pattern for state management
- **Async event loop** - tokio + crossterm for responsive TUI
- **ModelBackend trait** - Swappable inference backends

## Key Files

| File | Purpose |
|------|---------|
| `src/app.rs` | Application state (TEA Model) |
| `src/update.rs` | State transitions (TEA Update) |
| `src/tui/ui.rs` | Rendering (TEA View) |
| `src/model/backend.rs` | Model abstraction trait |
| `src/model/llama.rs` | llama_cpp implementation |
| `src/tui/event.rs` | Async event handling |

## Development

- **Always build with `--release`** - debug builds are 100x slower for LLM inference
- **Use `--features metal`** for GPU acceleration on macOS
- **Models go in `./models/`** - gitignored, ~14GB for Devstral Small 2

## Current Status

Phase 1: Basic chat TUI with streaming responses

## Tech Stack

- Rust 2021 edition
- llama_cpp (edgenai) for local inference
- ratatui + crossterm for TUI
- tokio for async runtime
- Devstral Small 2 (24B Q4) as default model
