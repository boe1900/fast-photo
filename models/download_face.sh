#!/bin/bash
# Download face detection and recognition ONNX models
# UltraFace (face detection) + MobileFaceNet (face embedding)

set -euo pipefail

MODELS_DIR="${1:-./models}"
mkdir -p "$MODELS_DIR"

echo "📥 Downloading face detection model (UltraFace)..."
if [ ! -f "$MODELS_DIR/face-detect.onnx" ]; then
    curl -L -o "$MODELS_DIR/face-detect.onnx" \
        "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/ultraface/models/version-RFB-320.onnx"
    echo "✅ Face detection model downloaded"
else
    echo "⏭️  Face detection model already exists"
fi

echo "📥 Downloading face embedding model (MobileFaceNet)..."
if [ ! -f "$MODELS_DIR/face-embed.onnx" ]; then
    curl -L -o "$MODELS_DIR/face-embed.onnx" \
        "https://github.com/onnx/models/raw/main/validated/vision/body_analysis/arcface/model/arcfaceresnet100-11-int8.onnx"
    echo "✅ Face embedding model downloaded"
else
    echo "⏭️  Face embedding model already exists"
fi

echo ""
echo "🎉 Face models ready in $MODELS_DIR/"
ls -lh "$MODELS_DIR"/face-*.onnx
