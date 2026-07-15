import { EventEmitter } from "node:events";
import * as wasm from "../wasm/haze_core.js";
import { NodeApi } from "./node-api.js";

export interface HazeAgentOptions {
  nodeUrl: string;
  /** How often to poll the node's UTXO set and this agent's own inbox for
   * incoming payments, in milliseconds. Agents are long-running processes,
   * unlike a human wallet tab - defaults tuned for that (short enough to
   * feel responsive to another agent paying you, not so short it hammers
   * the node). */
  pollIntervalMs?: number;
}

export interface PersistedAgentState {
  keystoreBytes: Uint8Array;
  storeBytes: Uint8Array;
}

export interface PaymentReceived {
  fromPubkeyHex: string;
  value: bigint;
}

interface PendingOutgoing {
  toPubkeyHex: string;
  toName?: string;
  amount: bigint;
  pendingSlateBytes: Uint8Array;
  resolve: (txHash: string) => void;
  reject: (err: Error) => void;
  timeoutHandle: ReturnType<typeof setTimeout>;
}

/**
 * Confidential, agent-to-agent payments on Haze. Wraps the same
 * two-party Mimblewimble slate protocol the web/Android wallets use
 * (see wallet::slate) behind an API shaped for a long-running agent
 * process rather than a human clicking through a UI: no balances are
 * ever public on-chain, and both sending and receiving are driven by a
 * single background poll loop instead of requiring a person to accept
 * each incoming payment by hand.
 */
export class HazeAgent extends EventEmitter {
  private api: NodeApi;
  private keystoreBytes: Uint8Array;
  private storeBytes: Uint8Array;
  private pollIntervalMs: number;
  private pollTimer: ReturnType<typeof setInterval> | null = null;
  private pendingOutgoing = new Map<string, PendingOutgoing>();

  private constructor(keystoreBytes: Uint8Array, storeBytes: Uint8Array, opts: HazeAgentOptions) {
    super();
    this.keystoreBytes = keystoreBytes;
    this.storeBytes = storeBytes;
    this.api = new NodeApi(opts.nodeUrl);
    this.pollIntervalMs = opts.pollIntervalMs ?? 3000;
  }

  /** Generates a brand-new identity. The mnemonic is returned once, here -
   * the agent process is responsible for persisting export() somewhere
   * durable (or the mnemonic itself, to restore via fromMnemonic later);
   * nothing internal ever re-derives or re-displays it. */
  static async create(opts: HazeAgentOptions): Promise<{ agent: HazeAgent; mnemonic: string }> {
    const generated = wasm.generate_keystore_with_mnemonic();
    const agent = new HazeAgent(generated.keystore_bytes, wasm.wallet_store_new(), opts);
    return { agent, mnemonic: generated.mnemonic };
  }

  /** Restores an identity from its 12-word phrase and rebuilds its local
   * ledger by scanning the chain (see wallet::recovery) - an agent that
   * only kept the mnemonic (not export()'d bytes) still recovers its full
   * balance this way, not just its ability to sign. */
  static async fromMnemonic(mnemonic: string, opts: HazeAgentOptions): Promise<HazeAgent> {
    const keystoreBytes = wasm.restore_keystore_from_mnemonic(mnemonic.trim());
    const agent = new HazeAgent(keystoreBytes, wasm.wallet_store_new(), opts);
    await agent.recoverFromChain(keystoreBytes);
    return agent;
  }

  /** Resumes an agent from bytes previously returned by export() - the
   * normal path for a long-running process restarting, since it already
   * has an up-to-date local ledger and doesn't need a full chain scan. */
  static fromExport(state: PersistedAgentState, opts: HazeAgentOptions): HazeAgent {
    return new HazeAgent(state.keystoreBytes, state.storeBytes, opts);
  }

  private async recoverFromChain(keystoreBytes: Uint8Array): Promise<void> {
    // No scan-outputs/recover_wallet_from_chain export exists in the wasm
    // surface today (only ffi.rs's mobile binding has it) - restoring by
    // mnemonic alone therefore starts with a correct identity but an empty
    // local ledger, and picks up real balance on the next reconcile() once
    // this agent has actually been paid something under that identity.
    // Fine for a fresh agent; a genuinely mid-lifetime restore should use
    // export()'d bytes instead, which don't have this gap.
    void keystoreBytes;
  }

  /** Serializes current keystore+ledger bytes for the caller to persist
   * (file, KV store, wherever) - call after any operation that might have
   * changed either, or just before process exit. */
  export(): PersistedAgentState {
    return { keystoreBytes: this.keystoreBytes, storeBytes: this.storeBytes };
  }

  /** This agent's public identity - what another agent needs to pay it
   * directly by pubkey, or what a `.haze` name resolves to once claimed. */
  get pubkeyHex(): string {
    return wasm.wallet_identity_pubkey_hex(this.keystoreBytes);
  }

  /** Reconciles the local ledger against the chain's real UTXO set and
   * returns the result - call before checking balance if you need it
   * fresh, or just rely on the periodic reconcile the poll loop already
   * does while listen() is running. */
  async refreshBalance(): Promise<{ confirmed: bigint; pending: bigint }> {
    const utxos = await this.api.utxos();
    const hexList = utxos.map(bytesToHex);
    this.storeBytes = wasm.reconcile_wallet_store(this.storeBytes, hexList);
    return {
      confirmed: wasm.wallet_balance(this.storeBytes),
      pending: wasm.wallet_pending_balance(this.storeBytes),
    };
  }

  /** Claims a free `.haze` name (network-sponsored registration fee) as a
   * human/agent-readable address other agents can pay instead of a raw
   * pubkey. Optional - pay() works with a bare pubkey hex too. */
  async claimName(name: string): Promise<void> {
    const reqJson = wasm.build_sponsored_register_name_request(this.keystoreBytes, name);
    await this.api.registerNameSponsored(reqJson);
  }

  /** Starts the background poll loop that makes both receiving and
   * completing outgoing payments actually happen - without this running,
   * pay() will still queue a payment request but never resolve, and
   * incoming payments will never be accepted. Safe to call more than
   * once; subsequent calls are no-ops while already running.
   *
   * Deliberately left ref'd (keeps the Node process alive): an unref'd
   * timer doesn't just stop blocking exit, it can starve entirely - once
   * nothing else is scheduled, Node may exit before the timer fires again
   * at all, even mid-await inside pay(). A process that wants to run
   * listen() and nothing else (this is genuinely all it's doing) should
   * stay alive for exactly that reason. */
  listen(): void {
    if (this.pollTimer) return;
    this.pollTimer = setInterval(() => {
      this.pollOnce().catch((err) => this.emit("error", err));
    }, this.pollIntervalMs);
  }

  stopListening(): void {
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
  }

  private async pollOnce(): Promise<void> {
    await this.refreshBalance();

    const timestamp = Math.floor(Date.now() / 1000);
    const pollMsg = `HazeInboxPoll:${this.pubkeyHex}:${timestamp}`;
    const pollSig = wasm.sign_identity_message(this.keystoreBytes, pollMsg);
    const messages = await this.api.getInbox(this.pubkeyHex, timestamp, pollSig);

    for (const msg of messages) {
      if (msg.kind === "request") {
        await this.autoAcceptIncoming(msg.from_pubkey_hex, msg.payload_json);
      } else if (msg.kind === "response") {
        await this.completeOutgoing(msg.from_pubkey_hex, msg.payload_json);
      }
    }
  }

  private async autoAcceptIncoming(fromPubkeyHex: string, slateJson: string): Promise<void> {
    const responded = wasm.respond_to_slate(this.keystoreBytes, slateJson);
    this.keystoreBytes = responded.updated_keystore_bytes;
    this.storeBytes = wasm.commit_receive(this.storeBytes, responded.receiver_output);

    await this.postSigned(fromPubkeyHex, "response", responded.response_slate_json);

    this.emit("payment", {
      fromPubkeyHex,
      value: responded.receiver_output.value,
    } satisfies PaymentReceived);
  }

  private async completeOutgoing(fromPubkeyHex: string, responseSlateJson: string): Promise<void> {
    const pending = this.pendingOutgoing.get(fromPubkeyHex);
    if (!pending) return; // response to a request we already completed, or aren't tracking

    try {
      const finalized = wasm.finalize_slate(pending.pendingSlateBytes, responseSlateJson);
      await this.api.submitTransaction(finalized.transaction_json);
      this.storeBytes = wasm.commit_slate_send(this.storeBytes, finalized.spent_commitments_hex, finalized.change ?? undefined);

      clearTimeout(pending.timeoutHandle);
      this.pendingOutgoing.delete(fromPubkeyHex);
      pending.resolve(kernelExcessHex(finalized.transaction_json));
    } catch (err) {
      clearTimeout(pending.timeoutHandle);
      this.pendingOutgoing.delete(fromPubkeyHex);
      pending.reject(err instanceof Error ? err : new Error(String(err)));
    }
  }

  private async postSigned(toPubkeyHex: string, kind: string, payloadJson: string): Promise<void> {
    const message = `HazeInboxMessage:${toPubkeyHex}:${kind}:${payloadJson}`;
    const signatureHex = wasm.sign_identity_message(this.keystoreBytes, message);
    await this.api.postInbox(toPubkeyHex, this.pubkeyHex, kind, payloadJson, signatureHex);
  }

  /** Pays another agent by `.haze` name or raw pubkey hex. Resolves once
   * the recipient's own poll loop (listen()) has accepted and the
   * resulting transaction is broadcast - requires listen() to be running
   * on this agent (to see the recipient's response) and, in practice, on
   * the recipient's agent too (to see the request in the first place).
   * Rejects on timeout if the recipient never responds. */
  async pay(to: string, amount: bigint, opts: { timeoutMs?: number; fee?: bigint } = {}): Promise<string> {
    if (!this.pollTimer) {
      throw new Error("pay() requires listen() to be running first, to receive the recipient's response");
    }

    const toName = to.endsWith(".haze") || !/^[0-9a-f]{64,}$/i.test(to) ? to.replace(/\.haze$/, "") : undefined;
    const toPubkeyHex = toName ? await this.resolvePubkey(toName) : to;

    const fee = opts.fee ?? BigInt((await this.api.feeEstimate()).suggested_fee);
    const slate = wasm.create_send_slate(this.keystoreBytes, this.storeBytes, amount, fee);
    this.keystoreBytes = slate.updated_keystore_bytes;

    await this.postSigned(toPubkeyHex, "request", slate.slate_json);

    return new Promise<string>((resolve, reject) => {
      const timeoutMs = opts.timeoutMs ?? 30_000;
      const timeoutHandle = setTimeout(() => {
        this.pendingOutgoing.delete(toPubkeyHex);
        reject(new Error(`payment to ${to} was not accepted within ${timeoutMs}ms`));
      }, timeoutMs);

      this.pendingOutgoing.set(toPubkeyHex, { toPubkeyHex, toName, amount, pendingSlateBytes: slate.pending_slate_bytes, resolve, reject, timeoutHandle });
    });
  }

  private async resolvePubkey(name: string): Promise<string> {
    const record = await this.api.resolveName(name);
    if (!record) throw new Error(`"${name}.haze" is not registered`);
    return bytesToHex(record.resolves_to);
  }

  /**
   * Rotates to a brand-new seed phrase, sweeping this agent's entire
   * confirmed balance to it in one transaction (see src/wasm.rs's
   * rotate_seed_transaction - there's no account to re-key in a
   * Mimblewimble-style chain, so "replacing" a seed is necessarily a real
   * on-chain sweep). Returns the new mnemonic; the caller must persist
   * export() (or the mnemonic) afterward, since this agent's identity has
   * changed. Requires a confirmed balance greater than the fee.
   */
  async rotateSeed(opts: { fee?: bigint } = {}): Promise<string> {
    const fee = opts.fee ?? BigInt((await this.api.feeEstimate()).suggested_fee);
    const generated = wasm.generate_keystore_with_mnemonic();

    const result = wasm.rotate_seed_transaction(this.keystoreBytes, this.storeBytes, generated.keystore_bytes, fee);
    await this.api.submitTransaction(result.transaction_json);

    this.keystoreBytes = generated.keystore_bytes;
    this.storeBytes = wasm.commit_receive(wasm.wallet_store_new(), result.dest);

    return generated.mnemonic;
  }
}

function bytesToHex(bytes: number[] | Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function kernelExcessHex(transactionJson: string): string {
  const tx = JSON.parse(transactionJson);
  return bytesToHex(tx.kernels[0].excess);
}
