# Tachyon-Tex Performance Methodology

To achieve sub-second LaTeX compilation, we target three main areas of optimization:

## 1. Engine Warmup & Format Pre-loading
Standard LaTeX (`pdflatex`) parses `\documentclass` and `\usepackage` on every run. Tectonic allows us to pre-generate format files and bundle them.
- **Goal**: Reach <100ms for standard article templates.
- **Status**: Implemented via `warmup.tex` in Docker build.

## 2. In-Memory Filesystem (VFS)
I/O is the enemy. By using `tectonic::io::memory::MemoryIo`, we eliminate the syscall overhead of writing `.aux`, `.log`, and `.pdf` files to disk.
- **Goal**: Eliminate SSD/HDD latency (2-10ms saved).
- **Status**: Implemented in `src/main.rs`.

## 3. Persistent Package Bundle
Downloading packages on-the-fly (Tectonic's default behavior) is slow for real-time needs.
- **Implementation**: The bundle is loaded into `AppState` once at startup and shared across all requests.
- **Status**: Implemented with `Arc<BundleCache>`.

## 4. Zero-Process Overhead
By linking Tectonic as a library, we avoid `fork()`/`exec()` calls. The compilation runs in the same address space as the web server.
- **Status**: Implemented via embedded Crate integration.

## Benchmarking Strategy
We will measure:
1. **Engine Time**: Pure compilation time reported by the engine.
2. **Transfer Time**: ZIP upload + PDF download time.
3. **Total RTT**: Time as perceived by the client.

| Metric | Target | Current |
|--------|--------|---------|
| Simple Doc | < 100ms | TBD |
| Complex Doc (TIKZ) | < 300ms | TBD |
| multi-pass (BibTeX) | < 500ms | TBD |
