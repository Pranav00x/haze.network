# Haze 🌫️

Haze is a lightweight, privacy-preserving Layer-1 blockchain built on the **Mimblewimble** protocol. Designed as the perfect middle ground between transparent but lightweight nodes and fully private but heavy networks, Haze makes privacy-preserving node operation easy without compromising the cryptographic guarantees of Mimblewimble.

## Features

- **Pedersen Commitments**: Hides transaction amounts while retaining the ability to homomorphically verify balances ($C = v \cdot H + r \cdot G$). We use `curve25519-dalek` and the Ristretto group to safely avoid small-subgroup cofactor attacks.
- **Bulletproofs Range Proofs**: Proves that committed transaction values are strictly positive ($\ge 0$) without revealing the values or requiring a trusted setup.
- **Schnorr Signatures**: Kernel signatures generated via Fiat-Shamir transcripts (`merlin`) prove ownership of excess blinding factors.
- **Cut-Through**: Compresses the blockchain history by canceling out matching spent-input and created-output pairs within the same block or transaction pool.
- **Dandelion++ Routing**: Stem-then-fluff P2P broadcast mechanism to obscure the origin IP address of transactions.

## Architecture

The project is built in Rust to prioritize correctness, safety, and performance.
- `src/core/`: Contains the state engine, block structures, chain validation logic (UTXO management), and the transaction Cut-Through mechanics.
- `src/crypto/`: Houses the foundational cryptographic primitives (Pedersen Commitments, Bulletproofs wrappers, Schnorr signatures).
- `src/p2p/`: Asynchronous networking layer (powered by `tokio`) utilizing the Dandelion++ routing state machine.

## Getting Started

### Prerequisites
- **Rust Toolchain**: `rustup default stable`
- **C/C++ Build Tools**: Since Haze relies on `bulletproofs` (which uses memory zeroizing C scripts), you must have a C compiler installed locally.
  - *Windows*: Install Visual Studio Build Tools with the "Desktop development with C++" workload.
  - *Linux/macOS*: Install `build-essential` or `gcc`/`clang`.

### Building
```bash
git clone https://github.com/Pranav00x/haze.git
cd haze
cargo build --release
```

### Testing
```bash
cargo test
```

## Status
Haze is currently in active development targeting a testnet launch. The core Mimblewimble cryptographic foundations and Chain State mechanics have been implemented.
