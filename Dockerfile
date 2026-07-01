### Builder ###
# "bookworm" (latest stable, not pinned to a specific minor version) - the
# project's edition = "2024" needs Rust 1.85+.
FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y build-essential && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .

# Only the node/wallet binary is needed at runtime - restricting the build to
# this target avoids also compiling the cdylib/staticlib UniFFI artifacts
# added for the mobile wallet core (src/ffi.rs), which aren't used here.
RUN cargo build --release --bin haze

### Runtime ###
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/haze /app/haze

EXPOSE 8332 8333

# Shell form (not exec-form array) so ${PORT} is resolved at container start -
# platforms like Render assign their own port via a PORT env var rather than
# letting you pick one; this falls back to 8332 when PORT isn't set (e.g.
# running locally with `docker run`, or on Fly.io).
ENTRYPOINT ["/bin/sh", "-c", "exec /app/haze node --bind 0.0.0.0:8333 --rpc-port ${PORT:-8332} --stake-key 42"]
