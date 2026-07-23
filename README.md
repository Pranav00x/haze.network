# haze

[![CI](https://github.com/Pranav00x/haze/actions/workflows/ci.yml/badge.svg)](https://github.com/Pranav00x/haze/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![rust](https://img.shields.io/badge/rust-2024_edition-orange)](Cargo.toml)
[![block height](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fhaze-b3l9.onrender.com%2Fv1%2Fstatus&query=%24.height&label=block%20height&color=success)](https://haze-b3l9.onrender.com/v1/status)
[![active validators](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fhaze-b3l9.onrender.com%2Fv1%2Fstatus&query=%24.active_validators&label=validators&color=success)](https://haze-b3l9.onrender.com/v1/status)
[![mempool](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fhaze-b3l9.onrender.com%2Fv1%2Fstatus&query=%24.mempool_size&label=mempool&color=blue)](https://haze-b3l9.onrender.com/v1/status)

### 👀 something's coming.

A privacy-first L1 built on Mimblewimble — no smart contracts, no accounts, no on-chain history for who paid who. NFTs, drops, and a trustless marketplace already run natively on it.

**Testnet drops soon.** Watch this space. The badges above are live — they query the running node's `/v1/status` directly, not a fixed number in this file.

---

## Architecture

```mermaid
graph TD
    subgraph PURE["Pure logic — zero native deps, compiles to wasm32 + every mobile ABI unmodified"]
        CRYPTO["crypto<br/>ristretto · bulletproofs · merlin"]
        CORE["core<br/>chain · block · transaction · mempool<br/>cut_through · registry · assets · collections"]
        WALLET["wallet<br/>keystore · planner · slate · note"]
    end
    subgraph NATIVE["Native only — feature = native"]
        P2P["p2p<br/>tcp / ws transport · dandelion++"]
        API["api<br/>warp HTTP+WS · explorer · faucet"]
        BIN["haze bin<br/>ffi.rs (uniffi) · wasm.rs (wasm-bindgen)"]
    end

    CRYPTO --> CORE
    CRYPTO --> WALLET
    CORE --> P2P
    CORE --> API
    WALLET --> API
    CORE --> BIN
    WALLET --> BIN
    P2P --> BIN

    API -->|HTTP / WS| WalletWeb["haze-wallet-web"]
    API -->|HTTP| Marketplace["nft-marketplace"]
    BIN -->|uniffi bindings| Mobile["Android · iOS"]
    BIN -->|wasm-bindgen| WalletWeb
```

## Block production

Deterministic single-proposer-per-height PoS — no leader election round trip, no BFT voting, the whole network computes the same answer independently:

```mermaid
sequenceDiagram
    participant Mem as Mempool
    participant P as "proposer(h)"
    participant Net as "Network (Dandelion++)"
    participant Peer as "Every other node"

    Note over P: proposer(h) = active_validators[H(h ‖ prev_hash) mod n]
    Mem->>P: pending txs + name/asset/collection ops
    P->>P: cut-through matching (input, output) pairs
    P->>P: assemble block, sign kernel, derive height reward
    P->>Net: broadcast NewBlock
    Net->>Peer: gossip (fluff — blocks skip the stem phase)
    Peer->>Peer: Σ C_in − Σ C_out − fee·H + reward·H ≟ Σ excess
    Peer->>Peer: verify every range proof + kernel signature
    Peer->>Peer: apply_block — extends the heaviest valid fork
```

## Cut-through, visually

```mermaid
graph LR
    O["Output created<br/>(v·H + r·G)"] --> S{"Later spent<br/>as an Input?"}
    S -->|"same commitment,<br/>same block or mempool"| X["Cancel — pair<br/>removed entirely"]
    S -->|"still unspent"| U["Stays in the<br/>live UTXO set"]
    X -.->|"no trace of the link<br/>between payer and payee"| Ghost["nothing to compact-<br/>through later"]
```

A Mimblewimble L1, devnet stage. This is a technical reference, not a pitch — it assumes you already know what a Pedersen commitment and a Fiat–Shamir transcript are.

## State model

UTXO set only. No accounts, no global balance ledger.

**Output** := `(C, π, ξ)`
- `C = v·H + r·G` — Pedersen commitment, ristretto255
- `π = Bulletproofs::RangeProof(v, r)` — proves `v ∈ [0, 2^64)`, no trusted setup
- `ξ = ChaCha20Poly1305(note_key, idx‖v)` — optional recoverable note (`wallet::note`)

**Kernel** := `(excess, fee, σ)`
- `excess = Σ r_in − Σ r_out`, committed to value 0
- `σ = Schnorr(excess_sk, fee_LE)` — Fiat–Shamir transcript via `merlin`

Validity, checked in `core::transaction::validate_with_reward`:
- `Σ C_in − Σ C_out − fee·H + reward·H ≟ Σ excess_i`
- every output: `RangeProof.verify(C, π)`
- every kernel: `σ.verify(fee_LE, excess)`

```mermaid
classDiagram
    class Output {
        +Commitment C
        +RangeProof π
        +Option~Note~ ξ
    }
    class Kernel {
        +Commitment excess
        +u64 fee
        +Signature σ
    }
    class Transaction {
        +Vec~Output~ inputs
        +Vec~Output~ outputs
        +Vec~Kernel~ kernels
    }
    class Block {
        +BlockHeader header
        +Transaction body
        +Vec~RegisterNameOp~ name_ops
        +Vec~MintAssetOp~ asset_ops
        +Vec~LaunchCollectionOp~ launch_ops
    }
    note for Output "C = v·H + r·G"
    note for Kernel "excess = Sum(r_in) - Sum(r_out)"
    Transaction "1" --> "*" Output : consumes/creates
    Transaction "1" --> "*" Kernel : proves
    Block "1" --> "1" Transaction : body
    Note --o Output : optional, recoverable
```

## Cut-through

Per-block and mempool-wide: any `(input, output)` pair on matching commitments cancels, regardless of arrival order. Horizon compaction (`core::compaction`, default 1000-block window) prunes spent in/out pairs below the horizon without invalidating tip-relative kernel-sum verification.

Compacted peers can't serve full historical re-validation past their horizon — see `PrunedRange` in `p2p::message` and `earliest_full_height()`.

## Consensus

PoS, deterministic proposer selection per height:

```
proposer(h) = active_validators[ H(h ‖ prev_hash) mod |active_validators| ]
```

A validator is a revealed `(commitment, value, blinding)` for an already-mined, currently-unspent output. There's no separate stake-lock UTXO type — spending that exact output retroactively deregisters the validator (`core::chain.rs`, `active_validators.retain` on input match). No slashing.

```mermaid
stateDiagram-v2
    [*] --> Unspent: output mined
    Unspent --> Active: RegisterValidatorOp<br/>reveals (commitment, value, blinding)
    Active --> Active: not selected this height
    Active --> Proposer: H(h ‖ prev_hash) mod n<br/>picks this commitment
    Proposer --> Active: block produced, height advances
    Active --> Deregistered: the backing output is spent
    Deregistered --> [*]
    note right of Deregistered
        No slashing — leaving is free,
        just costs future proposer weight
    end note
```

## Registries

Non-confidential, first-write-wins, separate namespaces:

| | module | ops |
|---|---|---|
| `.haze` names | `core::registry` | `RegisterNameOp` / `TransferNameOp` |
| assets ("NFTs") | `core::assets` | `MintAssetOp` / `TransferAssetOp` |

Both are committed into `BlockHeader` via a flat sorted-hash root (sort by key, concat, hash — not a Merkle tree, no membership proofs yet). Ownership is keyed by the wallet's stable `identity_key`, shared across both registries. Both deliberately skip Dandelion (see Network) — ownership is public by construction, there's no anonymity set to protect.

## Trustless swaps

No escrow contract — there isn't one to write, this chain has no smart contracts. Instead `TransferAssetOp` (and `MintAssetOp`, for collection-drop mints) carries an optional `required_kernel_excess: Commitment`, bound into the signed message. A seller can sign a transfer that's only valid *once a specific payment kernel exists on-chain* — so they can hand over a valid signature before being paid, and `apply_linear_block` simply won't apply it until that exact kernel excess shows up. Nobody ever custodies the asset or the payment on the other party's behalf.

```mermaid
sequenceDiagram
    participant Buyer
    participant Seller
    participant Chain as "apply_linear_block"

    Seller->>Seller: sign TransferAssetOp<br/>{ new_owner: Buyer, required_kernel_excess: K }
    Seller-->>Buyer: hand over the signed (but inert) transfer
    Note over Buyer,Seller: Buyer has a valid signature,<br/>but it does nothing yet
    Buyer->>Chain: broadcast payment tx with kernel excess K
    Buyer->>Chain: submit the signed TransferAssetOp
    Chain->>Chain: kernel K exists? --check--> yes
    Chain->>Chain: apply transfer + payment, same block
    Note over Chain: If K never lands, the transfer<br/>never applies - no refund needed, nothing moved
```

## Network

Transport-agnostic by construction (`p2p::transport::{PeerReader, PeerWriter}`):

- **Tcp** — raw length-prefixed bincode, `u32` LE prefix, ≤32 MiB/msg
- **WsServer** — `warp::ws()`, inbound-only, rides the node's existing HTTP(S) port (`GET /v1/p2p/ws`) — for hosts that only proxy one port
- **WsClient** — `tokio-tungstenite`, outbound-only, dialed when a `--peers` entry has a `ws(s)://` prefix

Payment gossip is Dandelion++ (20% fluff probability per hop, 15s fallback-fluff timer). A locally-originated tx enters the stem phase exactly like a relayed hop (`p2p::server::dispatch_dandelion_tx`) — flat-broadcasting a local tx would make the originating node trivially distinguishable from a relay, defeating the entire point.

```mermaid
graph LR
    Origin(["Originating node<br/>(indistinguishable from a relay)"]) -->|stem, 80%/hop| H1["Peer"]
    H1 -->|stem, 80%/hop| H2["Peer"]
    H2 -->|stem, 80%/hop| H3["Peer"]
    H3 -->|"fluff, 20%/hop<br/>or 15s timer fires"| Broadcast{{"Flat broadcast<br/>to the whole network"}}
    Broadcast --> P1["Peer"]
    Broadcast --> P2["Peer"]
    Broadcast --> P3["Peer"]
```

Sync: `Handshake → ChainInfo → GetBlocks(from_height) → BlocksBatch`, 256 blocks/round. Reorg via `rollback_block` + height-keyed `validator_snapshots`. `active_validators` isn't part of block history (mutated only by live `RegisterValidator`) — synced separately via `GetValidators`/`ValidatorsList` after block sync completes.

```mermaid
sequenceDiagram
    participant Us as New/catching-up node
    participant Peer as Existing peer

    Us->>Peer: Handshake { listen_addr }
    Peer->>Us: Handshake { listen_addr }
    Us->>Peer: ChainInfo request
    Peer->>Us: ChainInfo { height, tip_hash }
    loop until caught up, 256 blocks/round
        Us->>Peer: GetBlocks { from_height }
        Peer->>Us: BlocksBatch { blocks, has_more }
        Us->>Us: apply_block per block (validate + extend)
    end
    Us->>Peer: GetValidators
    Peer->>Us: ValidatorsList
    Note over Us: fully synced — active_validators<br/>isn't derivable from block history alone
```

## Crypto stack

`curve25519-dalek-ng` (ristretto group), `bulletproofs`, `merlin` (Fiat–Shamir transcripts), `sha2`, `chacha20poly1305`, `bip39`. `rustls` (aws-lc-rs backend) for the wasm/ws-client TLS path. Zero pairing-based crypto anywhere in the tree.

## Build surface

Cargo feature `native` gates every OS-dependent dep: `sled`, `warp`, `reqwest`, `clap`, `uniffi`, `tokio`, `tokio-tungstenite`, `futures-util`. Everything under `core::{chain,block,transaction,genesis,mempool,cut_through,registry,assets}`, `crypto::*`, and `wallet::{keystore,store,planner,slate}` is pure logic with zero native dependencies — the same source compiles to `wasm32-unknown-unknown` and every mobile ABI unmodified.

Targets exercised in CI / the release matrix:

- `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`
- `x86_64-apple-darwin`, `aarch64-apple-darwin`
- `aarch64-linux-android`, `armv7-linux-androideabi`, `{i686,x86_64}-linux-android`
- `wasm32-unknown-unknown`

```mermaid
graph LR
    SRC["core + crypto + wallet<br/>one source tree, zero native deps"]
    SRC --> LINUX["x86_64-unknown<br/>-linux-gnu"]
    SRC --> WIN["x86_64-pc<br/>-windows-msvc"]
    SRC --> MACX["x86_64 / aarch64<br/>-apple-darwin"]
    SRC --> AND["aarch64 / armv7 / i686 / x86_64<br/>-linux-android"]
    SRC --> WASM["wasm32-unknown<br/>-unknown"]
    LINUX --> BinL["haze node binary"]
    WIN --> BinW["haze node binary"]
    MACX --> BinM["haze node / desktop wallet"]
    AND -->|uniffi| Kotlin["Android wallet"]
    WASM -->|wasm-bindgen| JS["haze-wallet-web"]
```

## Threat model / known gaps

- No external security audit of the consensus code yet — the highest-leverage item before real value touches this chain.
- Effectively one node (`haze-b3l9.onrender.com`, no persistent disk) — not meaningfully decentralized yet; needs multiple independent validators on durable infra before "live" means anything.
- Genesis validator/faucet/vesting secrets are real out-of-band scalars, not present in this repo — one deliberate exception: the devnet genesis stake/claim output uses `blinding=42`, intentionally public (see `genesis.rs` module doc).
- Devnet. Resets happen without notice. Treat every balance as fake.
- Fungible multi-asset support was scoped **out** after analysis: a per-asset Pedersen generator scheme (`C = v·H_asset + r·G`) preserves the balance-equation security, but range-proof verification still needs the verifier to know which generator applies per output — i.e. a public per-output asset tag. That's a real confidentiality regression versus HAZE-only, not fixable within this dependency stack without a from-scratch Confidential-Assets-grade construction (blinded generator + surjection proof). NFTs (this repo's asset registry) don't have this problem: ownership was already public.
- Registry roots are flat hashes, not Merkle — no compact membership proofs for light clients yet.
- Block/tx propagation to a non-proposing peer works (Dandelion + mixed TCP/WS transport, verified live), but there's still no SPV/light-client sync mode — every node holds full (or horizon-compacted) chain state.
- `ChainState`'s `blocks`/`prune_meta` maps get their bodies pruned by cut-through/compaction, but the map entries themselves are never evicted — unbounded growth over the long run, not yet an issue at devnet scale.

```mermaid
quadrantChart
    title Known gaps, plotted by impact if left unaddressed vs. how soon they need attention
    x-axis Low urgency --> High urgency
    y-axis Low impact --> High impact
    quadrant-1 Fix before mainnet
    quadrant-2 Plan for post-launch
    quadrant-3 Accepted tradeoff / by design
    quadrant-4 Quick win
    No external audit: [0.85, 0.95]
    Single node, no persistent disk: [0.8, 0.9]
    No SPV / light-client mode: [0.35, 0.6]
    Flat non-Merkle registry roots: [0.3, 0.45]
    Unbounded blocks/prune_meta growth: [0.25, 0.4]
    No fungible confidential assets: [0.1, 0.35]
    Genesis secrets kept out-of-band: [0.15, 0.15]
    Devnet resets without notice: [0.1, 0.1]
```

Mitigated this session, for contrast — these started as the same kind of "known gap" and are now closed:

| gap | fix |
|---|---|
| Storage layer panicked on a serialize failure | `core::storage::StorageError`, propagated instead of `.unwrap()` |
| No per-IP request throttling on the public API | `api::ratelimit` — sliding-window cap in front of every route |
| No fuzzing of untrusted-byte parsing | `p2p/tests/deserialize_fuzz.rs` — random/mutated/truncated `P2pMessage` bytes |
| Wallet master seed never left memory on drop | `Keystore` now derives `ZeroizeOnDrop` |

## Verify

```
cargo build --release
cargo test --release                                    # 80 tests
wasm-pack build --target web --features wasm --no-default-features
cargo ndk -t arm64-v8a build --release --lib --no-default-features --features native
```

## License

MIT
