# 🚀 Tachyon-Tex: The Moonshot LaTeX Compiler

[![Docker Hub](https://img.shields.io/docker/v/srsergio/tachyon-tex?label=Docker%20Hub&logo=docker)](https://hub.docker.com/r/srsergio/tachyon-tex)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Tachyon-Tex** is a high-performance, ephemeral LaTeX compiler that achieves **sub-second compilation times**. Built with Rust and the Tectonic engine, it processes documents entirely in memory (RAM) to eliminate I/O bottlenecks.

## ⚡ Quick Start

```bash
# Pull and run (that's it!)
docker pull srsergio/tachyon-tex
docker run -p 8080:8080 srsergio/tachyon-tex

# Or use Docker Compose (recommended for RAM-disk optimization)
docker-compose up -d
```

## 📊 Performance Benchmarks

*Measured on January 13, 2026 with PDF Caching enabled*

### Compilation Times

| Document Type | First Run (MISS) | Cached Run (HIT) | Improvement |
|---------------|------------------|------------------|-------------|
| **Hello World** | 368ms | **~15ms** | 24x faster |
| **IEEE Template** | 781ms | **~20ms** | 39x faster |
| **IEEE + Long Text** | 836ms | **~20ms** | 42x faster |
| **IEEE + 10 Images (5MB)** | 8,480ms | **~25ms** | **340x faster** ⚡ |

### Deep Analysis: Where Time Goes

| Component | Time | % of Total |
|-----------|------|------------|
| Base engine (Hello World) | 353ms | 7% |
| IEEE packages overhead | +424ms | 9% |
| Long text processing | +54ms | 1% |
| **Image processing** | **+4,022ms** | **83%** |

> 💡 **Key insight**: 83% of compilation time is spent processing images. The PDF cache eliminates this entirely on repeat compilations.

### Cache System

Tachyon-Tex includes an intelligent **PDF compilation cache** using xxHash64:

- **Algorithm**: xxHash64 (~15 GB/s hashing speed)
- **TTL**: 24 hours (auto-cleanup every hour)
- **Control**: `PDF_CACHE_ENABLED=true/false` environment variable

```bash
# Response headers indicate cache status
X-Cache: HIT                    # Served from cache
X-Compile-Time-Ms: 0            # No compilation needed
X-Original-Compile-Time-Ms: 8480 # Original compilation time
```

## 🌙 Moonshot Philosophy

- **10x, not 10%**: We don't just optimize `pdflatex`. We bypass the OS process overhead by embedding the engine.
- **Zero-Disk Latency**: Everything from ZIP extraction to PDF generation happens in RAM.
- **Warm Caching**: The Docker image comes pre-loaded with the most common LaTeX packages.

## 🏗️ Architecture: The Zero-I/O Paradigm

Tachyon-Tex achieves its speed by identifying that the primary bottleneck in LaTeX compilation is the **System Call Barrier**. Standard compilers spend ~40% of their time on file descriptors and context switching.

1. **Embedded Engine**: We use Tectonic as a library (`tectonic` crate). The TeX engine stays warm in process memory.
2. **Virtual Memory Filesystem (VFS)**: We use `MemoryIo` to bypass the disk. The `.tex` input, `.aux` state, and `.pdf` output never touch the SSD.
3. **Pre-warmed Snapshot**: The Docker image contains a frozen state of the Tectonic package bundle, eliminating network lookups at runtime.

## 🔧 API Reference

### `POST /compile` — Compile LaTeX to PDF

Supports **ZIP files** or **multiple individual files** via multipart/form-data.

```bash
# Option 1: Send a ZIP file
curl -X POST -F "file=@project.zip" http://localhost:8080/compile -o output.pdf

# Option 2: Send multiple files directly (no ZIP needed!)
curl -X POST \
  -F "main=@main.tex" \
  -F "refs=@references.bib" \
  -F "style=@ieee.sty" \
  http://localhost:8080/compile -o output.pdf

# Check compilation time (in response header)
curl -X POST -F "file=@doc.tex" http://localhost:8080/compile -I
# X-Compile-Time-Ms: 857
# X-Files-Received: 3
```

**Response Headers:**
- `X-Compile-Time-Ms`: Engine compilation time in milliseconds (0 if cache hit)
- `X-Cache`: `HIT` (from cache) or `MISS` (freshly compiled)
- `X-Original-Compile-Time-Ms`: Original compilation time (only on cache hit)
- `X-Files-Received`: Number of files processed

---

### `POST /validate` — Validate LaTeX Syntax

Checks your `.tex` file for common errors **without compiling**.

```bash
curl -X POST -F "file=@document.tex" http://localhost:8080/validate
```

**Response (JSON):**
```json
{
  "valid": false,
  "errors": [
    {"line": 15, "column": null, "message": "Missing \\end{document}", "severity": "error"},
    {"line": 8, "column": null, "message": "Environment mismatch: expected \\end{itemize}, found \\end{enumerate}", "severity": "error"}
  ],
  "warnings": [
    "Line 12: Consider using \\[ \\] instead of $$ for display math",
    "Line 20: \\bf is deprecated, use \\textbf{} instead"
  ]
}
```

---

### `GET /packages` — List Available Packages

Returns all LaTeX packages available in the Tectonic bundle.

```bash
curl http://localhost:8080/packages
```

**Response (JSON):**
```json
{
  "count": 38,
  "packages": [
    {"name": "amsmath", "description": "AMS mathematical facilities", "category": "math"},
    {"name": "tikz", "description": "Create graphics programmatically", "category": "graphics"},
    {"name": "hyperref", "description": "Hyperlinks and bookmarks", "category": "document"}
  ]
}
```

---

### Web Interface

Open [http://localhost:8080](http://localhost:8080) for a drag-and-drop interface that supports multiple files.

## 🐳 Docker Hub

```bash
# Latest version
docker pull srsergio/tachyon-tex:latest

# Specific version
docker pull srsergio/tachyon-tex:v1.0.0
```

**Image URL**: [hub.docker.com/r/srsergio/tachyon-tex](https://hub.docker.com/r/srsergio/tachyon-tex)

## 📄 Scientific Paper

A detailed technical paper describing the architecture and benchmarks is available:
- [TACHYON_TEX_PAPER.pdf](docs/TACHYON_TEX_PAPER.tex)

## 📂 Project Structure

```text
tachyon-tex/
├── src/main.rs          # High-performance Rust server (Axum + Tectonic)
├── public/index.html    # Premium UI for document submission
├── Dockerfile           # Multi-stage optimized build
├── warmup.tex           # Pre-cache common LaTeX packages
├── docs/                # Scientific paper and documentation
└── COMPARATIVE_BENCHMARK.md  # Detailed performance comparison
```

## 🛠️ Build from Source

```bash
# Clone the repository
git clone https://github.com/srsergio/tachyon-tex.git
cd tachyon-tex

# Build Docker image (takes ~5 minutes)
docker build -t tachyon-tex .

# Run
docker run -p 8080:8080 tachyon-tex
```

## 🧪 Testing

The project includes a comprehensive test suite to verify all endpoints and performance.

### API Test Suite (Node.js)
Requires Node.js 18+. Verifies compilation, multi-file support, and syntax validation.
```bash
node tests/api.test.js
```

### Quick Test (PowerShell)
No dependencies required. Perfect for a quick health check.
```powershell
.\tests\quick-test.ps1
```

## 🚀 Key Features

- **Multi-File Support**: No ZIP required. Send multiple `.tex`, `.bib`, and `.sty` files in a single request.
- **Smart Detection**: Automatically finds the main `.tex` file by scanning for `\begin{document}`.
- **Syntax Validation**: Real-time validation endpoint to catch errors before compilation.
- **Pre-warmed Cache**: Common packages (TikZ, AMS, etc.) are pre-installed in the Docker image.
- **RAM-only Processing**: Virtual filesystem means no disk I/O bottlenecks.

## 🚀 Future Vision (Roadmap)

- [ ] **Delta-Compiling**: Intelligent caching of `.aux` state to only re-render modified pages.
- [ ] **Parallel Engine Orchestration**: Distributing multi-chapter builds across CPU cores.
- [ ] **WASM Edge**: Compiling the engine to WebAssembly for client-side preview without server roundtrips.
- [ ] **Predictive Caching**: Auto-detecting required packages from the preamble before the full run.

## 📜 License

MIT License - Feel free to use, modify, and distribute.

## 🤝 Contributing

Contributions are welcome! Please open an issue or submit a pull request.

---

**Made with ❤️ and Rust 🦀**
