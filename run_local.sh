#!/bin/bash
# Tachyon-Tex: Launcher

echo "🚀 Building the Tachyon-Tex orbital engine..."
docker build -t tachyon-tex .

if [ $? -eq 0 ]; then
    echo "✨ Build successful. Launching on :8080"
    echo "💡 Using /dev/shm for Zero-I/O compilation."
    docker run -p 8080:8080 --tmpfs /dev/shm:rw,size=512m tachyon-tex
else
    echo "❌ Build failed. Please check technical logs."
fi
