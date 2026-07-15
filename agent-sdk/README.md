# @haze/agent-sdk

Private, agent-to-agent payments on Haze.

AI agents that pay each other (or pay for API access, or split a cost) on a
transparent chain expose their entire counterparty and spending history to
everyone they transact with — every competitor, every customer, every other
agent. Haze is a Mimblewimble-style chain: balances and transaction graphs
are never public on-chain by design, not bolted on afterward. This SDK wraps
that in an API built for a long-running agent process, not a human clicking
through a wallet UI.

```ts
import { HazeAgent } from "@haze/agent-sdk";

const { agent, mnemonic } = await HazeAgent.create({ nodeUrl: "https://your-node" });
// persist `mnemonic` or agent.export() somewhere durable - this is the only
// time the mnemonic is ever available.

agent.listen(); // starts the background poll loop - required for both
                 // sending and receiving to actually complete

agent.on("payment", (p) => {
  console.log(`received ${p.value} from ${p.fromPubkeyHex}`);
});

const txHash = await agent.pay("other-agent.haze", 1_000n);
```

## Why a poll loop, not webhooks

Haze has no smart contracts and no push mechanism - agents (like the web and
Android wallets) participate in a two-party handshake over a plain HTTP
inbox relay (see `core::registry` / `api::inbox`). `listen()` runs that
handshake for you: it accepts incoming payment requests automatically (no
human needs to click "approve"), and completes your own outgoing payments
once the recipient's agent has responded. Both sides need `listen()` running
for a payment between them to actually settle - this mirrors exactly how
the web and Android wallets already work, just without a person in the loop.

## API

- `HazeAgent.create(opts)` - new identity, returns `{ agent, mnemonic }`.
- `HazeAgent.fromMnemonic(mnemonic, opts)` - restore from a 12-word phrase.
- `HazeAgent.fromExport(state, opts)` - resume from `agent.export()` bytes
  (the normal path for a long-running process restarting).
- `agent.pubkeyHex` - this agent's public identity.
- `agent.claimName(name)` - optional free `.haze` name, so other agents can
  pay `"you.haze"` instead of a raw pubkey.
- `agent.listen()` / `agent.stopListening()`.
- `agent.pay(to, amount, { timeoutMs?, fee? })` - `to` is a `.haze` name or
  raw pubkey hex. Resolves with the transaction's kernel excess hash once
  the recipient accepts; rejects on timeout.
- `agent.refreshBalance()` - `{ confirmed, pending }`.
- `agent.rotateSeed()` - generates a fresh identity and sweeps the entire
  confirmed balance to it in one transaction (see the wallet's seed
  rotation feature - same underlying `rotate_seed_transaction`). Returns
  the new mnemonic; persist `agent.export()` afterward.
- `agent.on("payment", (p: PaymentReceived) => …)` / `agent.on("error", …)`.

## Setup

```
npm install
npm run build
```

The compiled wasm bindings under `wasm/` are built from this repo's Rust
core (`wasm-pack build --target nodejs --features wasm --no-default-features`)
and checked in as-is, same as `haze-wallet-web/pkg/` - regenerate them after
any change to `src/wasm.rs`.

## Devnet status

This talks to whatever node you point `nodeUrl` at. Haze is public testnet
software - HAZE has no monetary value, and the chain may still be reset.
See `examples/two-agents.ts` for a full working demo (two independent
agents, real payment, run against a local devnet node).
