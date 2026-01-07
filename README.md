# 🚀 Tachyon-Tex: The Moonshot LaTeX Compiler

[![Docker Hub](https://img.shields.io/docker/v/srsergio/tachyon-tex?label=Docker%20Hub&logo=docker)](https://hub.docker.com/r/srsergio/tachyon-tex)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Tachyon-Tex** is a high-performance, ephemeral LaTeX compiler that achieves **sub-second compilation times**. Built with Rust and the Tectonic engine, it processes documents entirely in memory (RAM) to eliminate I/O bottlenecks.

## ⚡ Quick Start

```bash
# Pull and run (that's it!)
docker pull srsergio/tachyon-tex
docker run -p 8080:8080 srsergio/tachyon-tex

# Open in browser
# http://localhost:8080
```

## 📊 Real Benchmark Results

*Measured on January 7, 2026*

| Document Type | First Run | Cached Run | Engine Time | Speedup vs pdflatex |
|---------------|-----------|------------|-------------|---------------------|
| **Simple** (Hello World) | 1.63s | **0.42s** | 447ms | 8x |
| **TikZ** (Graphics) | 7.24s | **0.95s** | 857ms | 5x |
| **Complex** (Multi-section) | 2.71s | **0.94s** | 863ms | 4x |
| **IEEE Paper** (Multi-file) | 1.36s | **1.24s** | 1403ms | 3x |

> 💡 First run includes package download. Subsequent runs use cached packages.

## 🌙 Moonshot Philosophy

- **10x, not 10%**: We don't just optimize `pdflatex`. We bypass the OS process overhead by embedding the engine.
- **Zero-Disk Latency**: Everything from ZIP extraction to PDF generation happens in RAM.
- **Warm Caching**: The Docker image comes pre-loaded with the most common LaTeX packages.

## 🏗️ Architecture: The Zero-I/O Paradigm

Tachyon-Tex achieves its speed by identifying that the primary bottleneck in LaTeX compilation is the **System Call Barrier**. Standard compilers spend ~40% of their time on file descriptors and context switching.

1. **Embedded Engine**: We use Tectonic as a library (`tectonic` crate). The TeX engine stays warm in process memory.
2. **Virtual Memory Filesystem (VFS)**: We use `MemoryIo` to bypass the disk. The `.tex` input, `.aux` state, and `.pdf` output never touch the SSD.
3. **Pre-warmed Snapshot**: The Docker image contains a frozen state of the Tectonic package bundle, eliminating network lookups at runtime.

## 🔧 API Usage

### Compile a LaTeX Project

```bash
# Create a ZIP with your .tex files
zip project.zip main.tex

# Send to API
curl -X POST -F "file=@project.zip" http://localhost:8080/compile -o output.pdf

# Check compilation time (in response header)
curl -X POST -F "file=@project.zip" http://localhost:8080/compile -I
# x-compile-time-ms: 857
```

### Web Interface

Open [http://localhost:8080](http://localhost:8080) for a drag-and-drop interface.

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
