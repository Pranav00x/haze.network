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

# Only the RPC/HTTP port is exposed - the P2P port (8333, bound below) isn't
# reachable from the internet on Render's single-port web-service tier
# anyway, and exposing it caused Render's port-prober to send HTTP health
# checks straight into the P2P listener. The P2P protocol reads a connection's
# first 4 bytes as a message-length prefix, so an HTTP "HEAD ..." request gets
# read as a ~1.1GB length and rejected - Render then saw that as a failed
# health check and restarted the container on a loop, wiping the chain (no
# persistent disk yet) every time. Don't re-add 8333 here without also
# fixing how it's health-checked.
EXPOSE 8332

# Shell form (not exec-form array) so ${PORT} is resolved at container start -
# platforms like Render assign their own port via a PORT env var rather than
# letting you pick one; this falls back to 8332 when PORT isn't set (e.g.
# running locally with `docker run`, or on Fly.io).
#
# --stake-key follows HAZE_GENESIS_VALIDATOR_BLINDING (see
# core::genesis::genesis_validator_blinding), NOT hardcoded to the public
# devnet default 42 - this node has to operate AS whatever secret consensus
# considers the legitimate bootstrap validator, and those two values must
# match. Falls back to 42 when the env var is unset, so the existing devnet
# service (which never sets it) is unaffected; any deployment that DOES set
# HAZE_GENESIS_VALIDATOR_BLINDING (a real testnet/mainnet genesis) now
# actually runs as that real secret instead of silently reverting to the
# public one - hardcoding 42 here would have completely undone that fix for
# every real deployment built from this Dockerfile.
ENTRYPOINT ["/bin/sh", "-c", "exec /app/haze node --bind 0.0.0.0:8333 --rpc-port ${PORT:-8332} --stake-key ${HAZE_GENESIS_VALIDATOR_BLINDING:-42}"]
