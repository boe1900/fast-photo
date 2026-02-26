#!/bin/bash
# Download CLIP ViT-B-32 ONNX models from HuggingFace
# These are pre-split vision + text encoders from Qdrant

set -e

MODELS_DIR="$(dirname "$0")"
mkdir -p "$MODELS_DIR"

echo "📥 Downloading CLIP ViT-B-32 vision model..."
if [ ! -f "$MODELS_DIR/clip-vision.onnx" ]; then
    curl -L -o "$MODELS_DIR/clip-vision.onnx" \
        "https://huggingface.co/Qdrant/clip-ViT-B-32-vision/resolve/main/model.onnx"
    echo "✅ Vision model downloaded"
else
    echo "⏭️  Vision model already exists"
fi

echo "📥 Downloading CLIP ViT-B-32 text model..."
if [ ! -f "$MODELS_DIR/clip-text.onnx" ]; then
    curl -L -o "$MODELS_DIR/clip-text.onnx" \
        "https://huggingface.co/Qdrant/clip-ViT-B-32-text/resolve/main/model.onnx"
    echo "✅ Text model downloaded"
else
    echo "⏭️  Text model already exists"
fi

echo "📥 Downloading CLIP tokenizer..."
if [ ! -f "$MODELS_DIR/tokenizer.json" ]; then
    curl -L -o "$MODELS_DIR/tokenizer.json" \
        "https://huggingface.co/Qdrant/clip-ViT-B-32-text/resolve/main/tokenizer.json"
    echo "✅ Tokenizer downloaded"
else
    echo "⏭️  Tokenizer already exists"
fi

echo ""
echo "✅ All models downloaded to: $MODELS_DIR"
ls -lh "$MODELS_DIR"/*.onnx "$MODELS_DIR"/*.json 2>/dev/null
