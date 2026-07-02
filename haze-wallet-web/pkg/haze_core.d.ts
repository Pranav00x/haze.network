/* tslint:disable */
/* eslint-disable */

export class WasmOwnedOutput {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    commitment_hex: string;
    index: number;
    value: bigint;
}

export class WasmSendPlan {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get change(): WasmOwnedOutput | undefined;
    set change(value: WasmOwnedOutput | null | undefined);
    dest: WasmOwnedOutput;
    spent_commitments_hex: string[];
    transaction_json: string;
    updated_keystore_bytes: Uint8Array;
}

/**
 * Seeds the store with the well-known devnet genesis output (1,000,000,
 * blinding=42) - devnet-only convenience for funding a fresh web wallet,
 * mirrors the CLI's --claim-genesis. Only one wallet instance should do this.
 */
export function claim_genesis(store_bytes: Uint8Array): Uint8Array;

/**
 * Applies a previously-built SendPlan's effects to the wallet store. Must only be
 * called after the transaction was successfully broadcast.
 */
export function commit_send(store_bytes: Uint8Array, spent_commitments_hex: string[], dest: WasmOwnedOutput, change?: WasmOwnedOutput | null): Uint8Array;

/**
 * Generates a fresh keystore (random seed, via the browser's crypto.getRandomValues
 * through getrandom's "js" feature) and returns its serialized bytes.
 */
export function generate_keystore(): Uint8Array;

/**
 * Builds a real, self-contained transaction from the wallet's own confirmed UTXOs.
 * Allocates new output indices in the returned keystore bytes immediately (same
 * as the desktop wallet), regardless of whether the caller goes on to broadcast
 * successfully. The caller must POST `transaction_json` itself, then call
 * `commit_send` only on a successful response.
 */
export function plan_send(keystore_bytes: Uint8Array, store_bytes: Uint8Array, amount: bigint, fee: bigint): WasmSendPlan;

/**
 * Reconciles a wallet store's local ledger against the node's current on-chain
 * UTXO set (as returned by GET /v1/utxos, hex-encoded), returning updated bytes.
 */
export function reconcile_wallet_store(store_bytes: Uint8Array, chain_utxo_commitments_hex: string[]): Uint8Array;

/**
 * Confirmed (safely spendable) balance.
 */
export function wallet_balance(store_bytes: Uint8Array): bigint;

/**
 * Pending (unconfirmed) balance.
 */
export function wallet_pending_balance(store_bytes: Uint8Array): bigint;

/**
 * Creates an empty wallet store and returns its serialized bytes.
 */
export function wallet_store_new(): Uint8Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_get_wasmownedoutput_commitment_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmownedoutput_index: (a: number) => number;
    readonly __wbg_get_wasmownedoutput_value: (a: number) => bigint;
    readonly __wbg_get_wasmsendplan_change: (a: number) => number;
    readonly __wbg_get_wasmsendplan_dest: (a: number) => number;
    readonly __wbg_get_wasmsendplan_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_transaction_json: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_set_wasmownedoutput_commitment_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmownedoutput_index: (a: number, b: number) => void;
    readonly __wbg_set_wasmownedoutput_value: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmsendplan_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_dest: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_wasmownedoutput_free: (a: number, b: number) => void;
    readonly __wbg_wasmsendplan_free: (a: number, b: number) => void;
    readonly claim_genesis: (a: number, b: number) => [number, number, number, number];
    readonly commit_send: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly generate_keystore: () => [number, number];
    readonly plan_send: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number];
    readonly reconcile_wallet_store: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly wallet_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_pending_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_store_new: () => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
