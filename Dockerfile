FROM rust:bookworm AS builder

ENV PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libfontconfig1-dev \
    libgraphite2-dev \
    libharfbuzz-dev \
    libicu-dev \
    libssl-dev \
    zlib1g-dev \
    libfreetype6-dev \
    libpng-dev \
    cmake \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 1. Cache dependencies
COPY Cargo.toml Cargo.lock* ./
COPY rust-sdk ./rust-sdk
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# 2. Install Tectonic CLI for multi-file support
RUN cargo install tectonic --version 0.15.0 --features external-harfbuzz

# 3. Build the server
COPY . .
RUN touch src/main.rs && cargo build --release --features external-harfbuzz

# 4. Warmup
RUN mkdir -p /root/.cache/Tectonic && \
    /usr/local/cargo/bin/tectonic warmup.tex && \
    ./target/release/tachyon-tex --warmup || true

# --- STAGE 2: Final Image ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libfontconfig1 \
    libharfbuzz0b \
    libicu72 \
    libssl3 \
    libgraphite2-3 \
    ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/*

WORKDIR /app

# Copy our server binary
COPY --from=builder /app/target/release/tachyon-tex /usr/bin/tachyon-tex

# Copy Tectonic CLI for multi-file support
COPY --from=builder /usr/local/cargo/bin/tectonic /usr/bin/tectonic

# Copy Tectonic cache
COPY --from=builder /root/.cache/Tectonic /root/.cache/Tectonic

# Copy public static files
COPY --from=builder /app/public /app/public

ENV XDG_CACHE_HOME=/root/.cache

EXPOSE 8080

CMD ["/usr/bin/tachyon-tex"]
