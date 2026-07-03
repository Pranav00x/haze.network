/* tslint:disable */
/* eslint-disable */

export class WasmCreateSlateResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Keep this locally - never share it. Required by `finalize_slate` later.
     */
    pending_slate_bytes: Uint8Array;
    /**
     * Hand this to the recipient (out-of-band: chat, email, QR, etc).
     */
    slate_json: string;
    updated_keystore_bytes: Uint8Array;
}

export class WasmFinalizedTx {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get change(): WasmOwnedOutput | undefined;
    set change(value: WasmOwnedOutput | null | undefined);
    spent_commitments_hex: string[];
    transaction_json: string;
}

export class WasmKeystoreAndMnemonic {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    keystore_bytes: Uint8Array;
    /**
     * Only ever available right here at generation time - the keystore
     * itself never stores or re-derives it. The caller is responsible for
     * showing it to the user and requiring confirmation it's been saved.
     */
    mnemonic: string;
}

export class WasmOwnedOutput {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    commitment_hex: string;
    index: number;
    value: bigint;
}

export class WasmRecoveryResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    keystore_bytes: Uint8Array;
    recovered_balance: bigint;
    recovered_count: number;
    store_bytes: Uint8Array;
}

export class WasmRegisterNameResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get change(): WasmOwnedOutput | undefined;
    set change(value: WasmOwnedOutput | null | undefined);
    /**
     * POST this to /v1/names/register.
     */
    op_json: string;
    spent_commitments_hex: string[];
    updated_keystore_bytes: Uint8Array;
}

export class WasmRespondResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    receiver_output: WasmOwnedOutput;
    /**
     * Send this back to the original sender.
     */
    response_slate_json: string;
    updated_keystore_bytes: Uint8Array;
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
 * Builds a RegisterNameOp paying the registration fee from the wallet's own
 * confirmed UTXOs, signed with this wallet's stable naming identity key
 * (the same key every time - so `owner_pubkey` is consistent across
 * registrations from this wallet). The caller must POST `op_json`
 * themselves, then call `commit_register_name` only on success.
 */
export function build_register_name_request(keystore_bytes: Uint8Array, store_bytes: Uint8Array, name: string): WasmRegisterNameResult;

/**
 * Builds a sponsored registration request body for POST
 * /v1/names/register-sponsored - unlike build_register_name_request, this
 * needs no store/UTXOs/coin-selection at all, since the node's own faucet
 * reserve covers the flat registration fee (see FaucetState::
 * build_sponsored_fee_payment on the server side). This is what lets a
 * brand-new wallet register a name before it has ever received any funds.
 */
export function build_sponsored_register_name_request(keystore_bytes: Uint8Array, name: string): string;

/**
 * Builds a POST /v1/stake request body by staking the wallet's single
 * largest confirmed output. Fails if there is no confirmed output at least
 * `min_value`. Does not touch the store - staking doesn't spend anything.
 */
export function build_stake_request(keystore_bytes: Uint8Array, store_bytes: Uint8Array, min_value: bigint): string;

/**
 * Builds a TransferNameOp handing a name this wallet currently owns to a
 * new owner/resolution target, signed with this wallet's identity key. No
 * fee, no UTXO involved - the server rejects it if the signature doesn't
 * actually match the name's current on-chain owner. `new_resolves_to_hex`
 * is usually the same as `new_owner_pubkey_hex`, but kept separate to match
 * the underlying protocol (they're allowed to differ).
 */
export function build_transfer_name_request(keystore_bytes: Uint8Array, name: string, new_owner_pubkey_hex: string, new_resolves_to_hex: string): string;

/**
 * Seeds the store with the well-known devnet genesis output (1,000,000,
 * blinding=42) - devnet-only convenience for funding a fresh web wallet,
 * mirrors the CLI's --claim-genesis. Only one wallet instance should do this.
 */
export function claim_genesis(store_bytes: Uint8Array): Uint8Array;

/**
 * Receiver-side commit: adds the output from `respond_to_slate` to the
 * store as Pending. Optimistic (same tradeoff as the CLI) - there's no
 * callback confirming the sender actually broadcasts, so this is applied
 * right after responding rather than after on-chain confirmation.
 */
export function commit_receive(store_bytes: Uint8Array, output: WasmOwnedOutput): Uint8Array;

/**
 * Applies a previously-built name registration's effects (spent inputs,
 * optional change) to the store. Must only be called after the registration
 * was successfully queued via POST /v1/names/register.
 */
export function commit_register_name(store_bytes: Uint8Array, spent_commitments_hex: string[], change?: WasmOwnedOutput | null): Uint8Array;

/**
 * Applies a previously-built SendPlan's effects to the wallet store. Must only be
 * called after the transaction was successfully broadcast.
 */
export function commit_send(store_bytes: Uint8Array, spent_commitments_hex: string[], dest: WasmOwnedOutput, change?: WasmOwnedOutput | null): Uint8Array;

/**
 * Sender-side commit: applies a finalized+broadcast slate payment's effects
 * (spent inputs, optional change) to the store. Must only be called after
 * the transaction was successfully broadcast.
 */
export function commit_slate_send(store_bytes: Uint8Array, spent_commitments_hex: string[], change?: WasmOwnedOutput | null): Uint8Array;

/**
 * Sender step 1: builds a slate paying a different wallet `amount`. Returns
 * the slate JSON to hand to the recipient and the private pending-slate
 * bytes to keep locally until `finalize_slate`.
 */
export function create_send_slate(keystore_bytes: Uint8Array, store_bytes: Uint8Array, amount: bigint, fee: bigint): WasmCreateSlateResult;

/**
 * Sender step 2 (final): combines the local pending slate with the
 * recipient's response into the final Transaction. The caller must POST
 * `transaction_json` itself, then call `commit_slate_send` only on success.
 */
export function finalize_slate(pending_slate_bytes: Uint8Array, response_slate_json: string): WasmFinalizedTx;

/**
 * Generates a fresh keystore (random seed, via the browser's crypto.getRandomValues
 * through getrandom's "js" feature) and returns its serialized bytes.
 */
export function generate_keystore(): Uint8Array;

/**
 * Generates a fresh keystore backed by a real 12-word BIP39 mnemonic, so it
 * can be recovered later via restore_keystore_from_mnemonic().
 */
export function generate_keystore_with_mnemonic(): WasmKeystoreAndMnemonic;

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
 * Recovers a restored wallet's balance by trying to decrypt every note the
 * node hands back from GET /v1/scan-outputs (see api::explorer::
 * handle_scan_outputs and wallet::note) - a fresh restore has no local
 * record of which on-chain outputs are its own or what they're worth, since
 * a Pedersen commitment hides value and there's no local WalletStore left.
 * Only notes that decrypt successfully under this keystore's own note_key
 * AND are still present in `chain_utxo_commitments_hex` (i.e. unspent) are
 * added back as Confirmed - decrypting is already strong proof of
 * ownership (ChaCha20-Poly1305's auth tag), but the commitment is
 * recomputed from the recovered (index, value) as a final sanity check
 * before trusting it.
 */
export function recover_wallet_from_chain(keystore_bytes: Uint8Array, scan_entries_json: string, chain_utxo_commitments_hex: string[]): WasmRecoveryResult;

/**
 * Receiver step: fills in a slate received from a sender. Returns the
 * response JSON to send back, plus the output info the caller should add
 * to its own store as Pending.
 */
export function respond_to_slate(keystore_bytes: Uint8Array, slate_json: string): WasmRespondResult;

/**
 * Reconstructs a keystore from a previously-generated BIP39 phrase.
 */
export function restore_keystore_from_mnemonic(phrase: string): Uint8Array;

/**
 * Reveals the raw blinding factor (as hex) for the wallet's single largest
 * confirmed output - the private key needed to actually run a node as the
 * proposer for that staked output (`haze node --stake-key <hex>`). This is
 * sensitive: it's the spending key for that output, not just a view key.
 * Only exposed so a wallet holder can run their own validator; never sent
 * anywhere except directly into the user's own node process.
 */
export function reveal_stake_blinding_hex(keystore_bytes: Uint8Array, store_bytes: Uint8Array, min_value: bigint): string;

/**
 * Confirmed (safely spendable) balance.
 */
export function wallet_balance(store_bytes: Uint8Array): bigint;

/**
 * Derives this wallet's stable naming-registry identity pubkey (hex), so the
 * UI can show "your names resolve to this pubkey" without needing a
 * registration to already exist.
 */
export function wallet_identity_pubkey_hex(keystore_bytes: Uint8Array): string;

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
    readonly __wbg_get_wasmcreateslateresult_pending_slate_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmcreateslateresult_slate_json: (a: number) => [number, number];
    readonly __wbg_get_wasmcreateslateresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmfinalizedtx_change: (a: number) => number;
    readonly __wbg_get_wasmfinalizedtx_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmfinalizedtx_transaction_json: (a: number) => [number, number];
    readonly __wbg_get_wasmkeystoreandmnemonic_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmkeystoreandmnemonic_mnemonic: (a: number) => [number, number];
    readonly __wbg_get_wasmownedoutput_index: (a: number) => number;
    readonly __wbg_get_wasmownedoutput_value: (a: number) => bigint;
    readonly __wbg_get_wasmrecoveryresult_recovered_count: (a: number) => number;
    readonly __wbg_get_wasmregisternameresult_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmregisternameresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmrespondresult_receiver_output: (a: number) => number;
    readonly __wbg_get_wasmsendplan_change: (a: number) => number;
    readonly __wbg_get_wasmsendplan_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_transaction_json: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_set_wasmcreateslateresult_pending_slate_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmcreateslateresult_slate_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmcreateslateresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmfinalizedtx_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmfinalizedtx_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmfinalizedtx_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmownedoutput_commitment_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmownedoutput_index: (a: number, b: number) => void;
    readonly __wbg_set_wasmownedoutput_value: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmrecoveryresult_recovered_count: (a: number, b: number) => void;
    readonly __wbg_set_wasmregisternameresult_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmregisternameresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrespondresult_receiver_output: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_wasmcreateslateresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmfinalizedtx_free: (a: number, b: number) => void;
    readonly __wbg_wasmkeystoreandmnemonic_free: (a: number, b: number) => void;
    readonly __wbg_wasmownedoutput_free: (a: number, b: number) => void;
    readonly __wbg_wasmrecoveryresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmregisternameresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmrespondresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmsendplan_free: (a: number, b: number) => void;
    readonly build_register_name_request: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly build_sponsored_register_name_request: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly build_stake_request: (a: number, b: number, c: number, d: number, e: bigint) => [number, number, number, number];
    readonly build_transfer_name_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly claim_genesis: (a: number, b: number) => [number, number, number, number];
    readonly commit_receive: (a: number, b: number, c: number) => [number, number, number, number];
    readonly commit_register_name: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly commit_send: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly create_send_slate: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number];
    readonly finalize_slate: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly generate_keystore: () => [number, number];
    readonly generate_keystore_with_mnemonic: () => number;
    readonly plan_send: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number];
    readonly reconcile_wallet_store: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly recover_wallet_from_chain: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly respond_to_slate: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly restore_keystore_from_mnemonic: (a: number, b: number) => [number, number, number, number];
    readonly reveal_stake_blinding_hex: (a: number, b: number, c: number, d: number, e: bigint) => [number, number, number, number];
    readonly wallet_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_identity_pubkey_hex: (a: number, b: number) => [number, number, number, number];
    readonly wallet_pending_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_store_new: () => [number, number];
    readonly __wbg_get_wasmrecoveryresult_recovered_balance: (a: number) => bigint;
    readonly __wbg_get_wasmregisternameresult_change: (a: number) => number;
    readonly __wbg_set_wasmrecoveryresult_recovered_balance: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmregisternameresult_change: (a: number, b: number) => void;
    readonly __wbg_get_wasmsendplan_dest: (a: number) => number;
    readonly __wbg_set_wasmkeystoreandmnemonic_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmkeystoreandmnemonic_mnemonic: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrecoveryresult_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrecoveryresult_store_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmregisternameresult_op_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrespondresult_response_slate_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrespondresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_get_wasmrecoveryresult_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmrecoveryresult_store_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmrespondresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmownedoutput_commitment_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmregisternameresult_op_json: (a: number) => [number, number];
    readonly __wbg_get_wasmrespondresult_response_slate_json: (a: number) => [number, number];
    readonly __wbg_set_wasmsendplan_dest: (a: number, b: number) => void;
    readonly commit_slate_send: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
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
