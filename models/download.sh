#!/bin/bash
# Download Devstral Small 2 Q4 GGUF model
# Using bartowski's community conversion (no auth required, latest model)

set -e

MODEL_DIR="$(dirname "$0")"
MODEL_FILE="$MODEL_DIR/devstral-small-2-q4.gguf"

# bartowski's GGUF conversion of the latest Devstral-Small-2-24B-Instruct-2512
REPO_ID="bartowski/mistralai_Devstral-Small-2-24B-Instruct-2512-GGUF"
FILENAME="mistralai_Devstral-Small-2-24B-Instruct-2512-Q4_K_M.gguf"

if [ -f "$MODEL_FILE" ]; then
    echo "Model already downloaded at $MODEL_FILE"
    exit 0
fi

# Check for hf (brew) or huggingface-cli (pip)
if command -v hf &> /dev/null; then
    HF_CLI="hf"
elif command -v huggingface-cli &> /dev/null; then
    HF_CLI="huggingface-cli"
else
    echo "Error: Hugging Face CLI not found."
    echo ""
    echo "Install with Homebrew:"
    echo "  brew install huggingface-cli"
    echo ""
    echo "Or with pip:"
    echo "  pip install huggingface_hub"
    exit 1
fi

echo "Downloading Devstral Small 2 Q4 (~14GB)..."
echo "Source: $REPO_ID"
echo "This may take a while depending on your connection speed."
echo ""

# Download using hf cli
$HF_CLI download "$REPO_ID" "$FILENAME" --local-dir "$MODEL_DIR"

# Rename to expected filename
if [ -f "$MODEL_DIR/$FILENAME" ]; then
    mv "$MODEL_DIR/$FILENAME" "$MODEL_FILE"
    # Clean up .huggingface directory if created
    rm -rf "$MODEL_DIR/.huggingface" 2>/dev/null || true
    echo ""
    echo "Download complete: $MODEL_FILE"
    echo "Size: $(du -h "$MODEL_FILE" | cut -f1)"
    echo ""
    echo "You can now run: cargo run --release --features metal"
else
    echo "Error: Download failed. File not found: $MODEL_DIR/$FILENAME"
    echo ""
    echo "Try manual download:"
    echo "  $HF_CLI download $REPO_ID --include '*Q4_K_M*' --local-dir $MODEL_DIR"
    exit 1
fi
