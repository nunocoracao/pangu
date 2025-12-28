# Pangu

Terminal-based agentic coding assistant with local LLM inference.

## Quick Start

```bash
# 1. Build llama.cpp (one-time setup)
cd ../llama.cpp  # or clone from https://github.com/ggml-org/llama.cpp
mkdir build && cd build
cmake .. -DGGML_METAL=ON -DCMAKE_BUILD_TYPE=Release
make -j8 llama-server
cp bin/llama-server ../../pangu/bin/

# 2. Download the model
./models/download.sh

# 3. Build Pangu
cargo build --release

# 4. Run
./target/release/pangu --model models/devstral-small-2-q4.gguf
```

## Architecture

- **TEA (The Elm Architecture)** - Model/Update/View pattern for state management
- **Async event loop** - tokio + crossterm for responsive TUI
- **llama-server subprocess** - Spawns llama.cpp server for inference via HTTP
- **ModelBackend trait** - Swappable inference backends

## Key Files

| File | Purpose |
|------|---------|
| `src/app.rs` | Application state (TEA Model) |
| `src/update.rs` | State transitions (TEA Update) |
| `src/tui/ui.rs` | Rendering (TEA View) |
| `src/model/backend.rs` | Model abstraction trait |
| `src/model/llama_server.rs` | llama-server HTTP backend |
| `src/tui/event.rs` | Async event handling |
| `bin/llama-server` | llama.cpp server binary (built from source) |

## Development

- **Always build with `--release`** - debug builds are slower
- **llama-server must be in `./bin/`** - built from latest llama.cpp
- **Models go in `./models/`** - gitignored, ~14GB for Devstral Small 2

## Current Status

Phase 1: Basic chat TUI with streaming responses

## Tech Stack

- Rust 2021 edition
- llama.cpp (server mode) for local inference - supports mistral3 architecture
- ratatui + crossterm for TUI
- tokio + reqwest for async HTTP
- Devstral Small 2 (24B Q4) as default model
