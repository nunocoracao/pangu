#!/bin/bash
# Download Pangu model from Hugging Face
# Models available: devstral-small-2-q4.gguf (~12GB) and devstral-small-2-q6.gguf (~19GB)

set -e

MODEL_DIR="$(dirname "$0")"

# Default to Q4 (smaller), use Q6 for higher quality
MODEL_VARIANT="${1:-q4}"

case "$MODEL_VARIANT" in
    q4|Q4)
        FILENAME="devstral-small-2-q4.gguf"
        SIZE_HINT="~12GB"
        ;;
    q6|Q6)
        FILENAME="devstral-small-2-q6.gguf"
        SIZE_HINT="~19GB"
        ;;
    *)
        echo "Usage: $0 [q4|q6]"
        echo "  q4 - Smaller model (~12GB), faster"
        echo "  q6 - Higher quality (~19GB), more accurate"
        exit 1
        ;;
esac

MODEL_FILE="$MODEL_DIR/$FILENAME"
REPO_ID="nunocoracao/pangu"

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

echo "Downloading Pangu model ($MODEL_VARIANT, $SIZE_HINT)..."
echo "Source: https://huggingface.co/$REPO_ID"
echo "This may take a while depending on your connection speed."
echo ""

# Download using hf cli
$HF_CLI download "$REPO_ID" "$FILENAME" --local-dir "$MODEL_DIR"

# Clean up .huggingface directory if created
rm -rf "$MODEL_DIR/.huggingface" 2>/dev/null || true

if [ -f "$MODEL_FILE" ]; then
    echo ""
    echo "Download complete: $MODEL_FILE"
    echo "Size: $(du -h "$MODEL_FILE" | cut -f1)"
    echo ""
    echo "You can now run: cargo run --release"
else
    echo "Error: Download failed. File not found: $MODEL_FILE"
    echo ""
    echo "Try manual download from:"
    echo "  https://huggingface.co/$REPO_ID/resolve/main/$FILENAME"
    exit 1
fi
