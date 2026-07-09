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

export class WasmMerkleProofResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    leaf_index: number;
    proof_hex: string[];
    root_hex: string;
}

export class WasmMintAssetResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get change(): WasmOwnedOutput | undefined;
    set change(value: WasmOwnedOutput | null | undefined);
    /**
     * POST this to /v1/assets/mint.
     */
    op_json: string;
    spent_commitments_hex: string[];
    updated_keystore_bytes: Uint8Array;
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

export class WasmSlateReservation {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    get change(): WasmOwnedOutput | undefined;
    set change(value: WasmOwnedOutput | null | undefined);
    spent_commitments_hex: string[];
}

export class WasmSweepResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Add this to the wallet's own store as Pending on success (reuse
     * commit_send with an empty spent_commitments_hex and no change - the
     * swept reward inputs were never part of this wallet's own store to
     * begin with, only the destination output is new).
     */
    dest: WasmOwnedOutput;
    swept_count: number;
    swept_total: bigint;
    /**
     * POST this to /v1/transactions.
     */
    transaction_json: string;
    updated_keystore_bytes: Uint8Array;
}

/**
 * Patches a collection creator's approval signature (see
 * MintAssetOp::creator_signature and sign_collection_mint_approval) into an
 * already-built mint op_json (from build_collection_mint_asset_request),
 * without needing to rebuild it - rebuilding would re-select this wallet's
 * spendable UTXOs and could pick a different (conflicting) fee_payment.
 */
export function attach_creator_signature_to_mint(op_json: string, creator_signature_hex: string): string;

/**
 * Builds a signed cancellation for a listing this wallet previously
 * created - see POST /v1/marketplace/cancel.
 */
export function build_cancel_listing_request(keystore_bytes: Uint8Array, asset_id: string): string;

/**
 * Sibling to build_mint_asset_request for a mint claimed against a
 * collection's scheduled phase (see core::collections) - takes the same
 * spendable-UTXO-funded fee_payment path, plus the collection-drop fields.
 * `allowlist_proof_hex`/`allowlist_leaf_index` are only required when the
 * target phase actually has an allowlist_merkle_root (see
 * compute_allowlist_merkle_proof); `required_kernel_excess_hex`, if
 * provided, is the payment-conditioning primitive (same as
 * build_transfer_asset_request's) - typically supplied by the collection
 * creator's auto-responding wallet once it has independently verified an
 * incoming payment slate pays the phase's price, not built by the minter
 * themselves. A separate sibling (not a change to build_mint_asset_request)
 * so every existing plain-mint call site keeps working unchanged.
 */
export function build_collection_mint_asset_request(keystore_bytes: Uint8Array, store_bytes: Uint8Array, asset_id: string, metadata: string, fee: bigint, collection_id: string, phase_index: number, allowlist_proof_hex?: string[] | null, allowlist_leaf_index?: number | null, required_kernel_excess_hex?: string | null): WasmMintAssetResult;

/**
 * Builds a signed marketplace Listing (see core::marketplace) advertising
 * an asset this wallet owns for sale at `price`, signed with this wallet's
 * identity key - the same key the asset's owner_pubkey on-chain is
 * expected to match, checked server-side at POST /v1/marketplace/list.
 */
export function build_create_listing_request(keystore_bytes: Uint8Array, asset_id: string, price: bigint, listed_at: bigint): string;

/**
 * Builds a signed LaunchCollectionOp for a scheduled multi-phase NFT drop
 * (see core::collections) - `phases_json` is the JSON serialization of a
 * `Vec<MintPhase>` (the caller builds this array client-side: each phase
 * has `name`, `start_time`, `end_time`, `price`, `per_wallet_limit`, and
 * optional `allowlist_merkle_root` - for an allowlisted phase, compute the
 * root client-side first via `compute_allowlist_merkle_proof`'s root_hex,
 * or build it directly from a pubkey list; a Public/open phase omits the
 * root entirely). No fee_payment - launching costs nothing beyond ordinary
 * block-inclusion (see LaunchCollectionOp's own doc comment). The caller
 * must POST the returned JSON to /v1/collections/launch.
 */
export function build_launch_collection_request(keystore_bytes: Uint8Array, collection_id: string, name: string, symbol: string, metadata: string, phases_json: string, royalty_bps: number): string;

/**
 * Builds a MintAssetOp paying `fee` (must be >= ASSET_MINT_FEE) from the
 * wallet's own confirmed UTXOs, signed with this wallet's stable identity
 * key. `metadata` is free-form text (recommended shape: JSON
 * `{title, description, image}`) stored directly on-chain, bounded at
 * MAX_METADATA_BYTES - a real marketplace needs actual preview data, and
 * storing only a hash would make browsing depend on an external metadata
 * host, reintroducing exactly the trust dependency the atomic-swap design
 * is trying to eliminate. Callers should pass GET /v1/fee-estimate's
 * suggested_asset_fee rather than hardcoding ASSET_MINT_FEE, same reasoning
 * as build_register_name_request. The caller must POST `op_json`
 * themselves, then call `commit_mint_asset` only on success.
 */
export function build_mint_asset_request(keystore_bytes: Uint8Array, store_bytes: Uint8Array, asset_id: string, metadata: string, fee: bigint): WasmMintAssetResult;

/**
 * Builds a RegisterNameOp paying `fee` (must be >= NAME_REGISTRATION_FEE,
 * the hard consensus floor - see its doc comment for why the floor itself
 * can't be a live congestion-derived value) from the wallet's own confirmed
 * UTXOs, signed with this wallet's stable naming identity key (the same key
 * every time - so `owner_pubkey` is consistent across registrations from
 * this wallet). Callers should pass GET /v1/fee-estimate's
 * suggested_name_fee rather than hardcoding NAME_REGISTRATION_FEE, so
 * paying "the going rate" adapts to how busy the name-registration backlog
 * actually is. The caller must POST `op_json` themselves, then call
 * `commit_register_name` only on success.
 */
export function build_register_name_request(keystore_bytes: Uint8Array, store_bytes: Uint8Array, name: string, fee: bigint): WasmRegisterNameResult;

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
 * Builds a TransferAssetOp handing an asset this wallet currently owns to a
 * new owner's identity pubkey, signed with this wallet's identity key. No
 * fee, no UTXO involved - the server rejects it if the signature doesn't
 * actually match the asset's current on-chain owner.
 *
 * `required_kernel_excess_hex`, if provided, makes this the trustless
 * marketplace atomic-swap primitive: the transfer only becomes valid once a
 * transaction kernel with that exact excess exists on-chain (see
 * core::assets::TransferAssetOp::required_kernel_excess and
 * tx_kernel_excess_hex below, which a buyer uses to get this value from
 * their own finalized-but-not-yet-broadcast payment transaction). This
 * lets a seller sign a transfer before a buyer's payment lands, safely -
 * it's cryptographically inert until that payment is actually on-chain.
 */
export function build_transfer_asset_request(keystore_bytes: Uint8Array, asset_id: string, new_owner_pubkey_hex: string, required_kernel_excess_hex?: string | null, required_royalty_kernel_excess_hex?: string | null): string;

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
 * Applies a previously-built asset mint's effects (spent inputs, optional
 * change) to the store. Must only be called after the mint was successfully
 * queued via POST /v1/assets/mint. Identical bookkeeping to
 * commit_register_name - kept as its own function so the JS side has a
 * clearly-scoped call per feature.
 */
export function commit_mint_asset(store_bytes: Uint8Array, spent_commitments_hex: string[], change?: WasmOwnedOutput | null): Uint8Array;

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
 * Computes `target_pubkey_hex`'s Merkle inclusion proof against the full
 * plaintext allowlist `pubkeys_hex` (fetched from the off-chain allowlist
 * endpoint - see core::allowlist) - lets a minter (or the wallet UI
 * building a collection launch) get everything build_collection_mint_asset_request
 * needs without re-deriving the tree by hand. Returns an error if
 * target_pubkey_hex isn't actually present in the list.
 */
export function compute_allowlist_merkle_proof(pubkeys_hex: string[], target_pubkey_hex: string): WasmMerkleProofResult;

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
 * Sender-side: the inputs/change a pending slate has ALREADY selected at
 * create_send_slate time, before the recipient has even responded (see
 * PendingSlate.spent_commitments/change - both are fixed at planning time,
 * the interactive round-trip only adds the recipient's own signature
 * share). Lets a caller building a SECOND, independent slate off the same
 * wallet store (e.g. a royalty payment alongside a marketplace payment)
 * eagerly commit_slate_send this one's reservation first, so the second
 * selection can't pick the same commitment - the same UTXO-collision this
 * project has hit before whenever two payments were built back-to-back
 * against a store that hadn't yet been told about the first one's picks.
 */
export function pending_slate_reservation(pending_slate_bytes: Uint8Array): WasmSlateReservation;

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
 * Signs an allowlist publish (see core::allowlist::AllowlistEntry) so the
 * off-chain, best-effort allowlist gossip can be cross-checked against this
 * collection's registered creator_pubkey server-side. `pubkeys_hex` is the
 * full plaintext list being published for this collection/phase.
 */
export function sign_allowlist_publish(keystore_bytes: Uint8Array, collection_id: string, phase_index: number, pubkeys_hex: string[], published_at: bigint): string;

/**
 * The collection creator's side of the approval handshake (see
 * MintAssetOp::creator_signature's doc comment) - signs approval for one
 * specific (asset_id, collection_id, phase_index, required_kernel_excess,
 * owner_pubkey) combination. The creator's own wallet should independently
 * verify (against the phase's timing/allowlist/price and the actual
 * on-chain payment) before calling this - this function only produces the
 * signature, it doesn't validate anything itself.
 */
export function sign_collection_mint_approval(keystore_bytes: Uint8Array, asset_id: string, collection_id: string, phase_index: number, required_kernel_excess_hex: string, owner_pubkey_hex: string): string;

/**
 * Signs an arbitrary UTF-8 message with this wallet's identity key - used
 * by the standalone marketplace site's "connect wallet" handoff (see
 * haze-marketplace-web) to let the wallet prove control of its identity
 * pubkey over a marketplace-issued nonce, without the marketplace site ever
 * touching the wallet's keys directly.
 */
export function sign_identity_message(keystore_bytes: Uint8Array, message: string): string;

/**
 * Finds every still-unspent block reward this validator has ever earned
 * (see wallet::note::coinbase_blinding/coinbase_note_key and
 * core::proposer, which now derives coinbase blindings from the staking
 * secret instead of a discarded random one) and sweeps all of them into a
 * single new output in this wallet's own keystore - turning "provably mine
 * but nowhere to spend it from" into an ordinary, self-owned, spendable
 * balance. `stake_key_hex` is the same secret reveal_stake_blinding_hex
 * already exposes for running a validator node with. Errors if nothing
 * unswept is found, or if the total found doesn't even cover `fee`.
 */
export function sweep_validator_rewards(stake_key_hex: string, scan_entries_json: string, chain_utxo_commitments_hex: string[], keystore_bytes: Uint8Array, fee: bigint): WasmSweepResult;

/**
 * Extracts a finalized (but not necessarily yet broadcast) transaction's
 * kernel excess as hex - used by a marketplace buyer to learn the exact
 * value to send the seller in a "want_transfer" inbox message, so the
 * seller can build a TransferAssetOp conditioned on this specific payment
 * (see build_transfer_asset_request's required_kernel_excess_hex). Every
 * Haze transaction has exactly one kernel by construction (see
 * wallet::slate::finalize_slate/wallet::planner::plan_send), so this
 * always reads kernels[0].
 */
export function tx_kernel_excess_hex(transaction_json: string): string;

/**
 * Verifies a signature produced by sign_identity_message - lets the
 * marketplace site check a "connect wallet" handoff's proof-of-pubkey
 * client-side, with no server round-trip and no key material involved.
 */
export function verify_identity_signature(pubkey_hex: string, message: string, signature_hex: string): boolean;

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
    readonly __wbg_get_wasmmerkleproofresult_leaf_index: (a: number) => number;
    readonly __wbg_get_wasmmerkleproofresult_proof_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmmintassetresult_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmmintassetresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmownedoutput_value: (a: number) => bigint;
    readonly __wbg_get_wasmrecoveryresult_recovered_count: (a: number) => number;
    readonly __wbg_get_wasmrespondresult_receiver_output: (a: number) => number;
    readonly __wbg_get_wasmsendplan_change: (a: number) => number;
    readonly __wbg_get_wasmsendplan_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_transaction_json: (a: number) => [number, number];
    readonly __wbg_get_wasmsendplan_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmslatereservation_spent_commitments_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmsweepresult_swept_count: (a: number) => number;
    readonly __wbg_get_wasmsweepresult_swept_total: (a: number) => bigint;
    readonly __wbg_get_wasmsweepresult_transaction_json: (a: number) => [number, number];
    readonly __wbg_get_wasmsweepresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_set_wasmcreateslateresult_pending_slate_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmcreateslateresult_slate_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmcreateslateresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmfinalizedtx_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmfinalizedtx_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmfinalizedtx_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmmerkleproofresult_leaf_index: (a: number, b: number) => void;
    readonly __wbg_set_wasmmerkleproofresult_proof_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmmintassetresult_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmmintassetresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmownedoutput_commitment_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmownedoutput_value: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmrecoveryresult_recovered_count: (a: number, b: number) => void;
    readonly __wbg_set_wasmrespondresult_receiver_output: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmsendplan_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsendplan_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmslatereservation_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsweepresult_swept_count: (a: number, b: number) => void;
    readonly __wbg_set_wasmsweepresult_swept_total: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmsweepresult_transaction_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmsweepresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_wasmcreateslateresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmfinalizedtx_free: (a: number, b: number) => void;
    readonly __wbg_wasmkeystoreandmnemonic_free: (a: number, b: number) => void;
    readonly __wbg_wasmmerkleproofresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmmintassetresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmownedoutput_free: (a: number, b: number) => void;
    readonly __wbg_wasmrecoveryresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmregisternameresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmrespondresult_free: (a: number, b: number) => void;
    readonly __wbg_wasmsendplan_free: (a: number, b: number) => void;
    readonly __wbg_wasmslatereservation_free: (a: number, b: number) => void;
    readonly __wbg_wasmsweepresult_free: (a: number, b: number) => void;
    readonly attach_creator_signature_to_mint: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly build_cancel_listing_request: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly build_collection_mint_asset_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: bigint, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number) => [number, number, number];
    readonly build_create_listing_request: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number, number];
    readonly build_launch_collection_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => [number, number, number, number];
    readonly build_mint_asset_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: bigint) => [number, number, number];
    readonly build_register_name_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: bigint) => [number, number, number];
    readonly build_sponsored_register_name_request: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly build_stake_request: (a: number, b: number, c: number, d: number, e: bigint) => [number, number, number, number];
    readonly build_transfer_asset_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number, number];
    readonly build_transfer_name_request: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
    readonly claim_genesis: (a: number, b: number) => [number, number, number, number];
    readonly commit_mint_asset: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly commit_receive: (a: number, b: number, c: number) => [number, number, number, number];
    readonly commit_send: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly compute_allowlist_merkle_proof: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly create_send_slate: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number];
    readonly finalize_slate: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly generate_keystore: () => [number, number];
    readonly generate_keystore_with_mnemonic: () => number;
    readonly pending_slate_reservation: (a: number, b: number) => [number, number, number];
    readonly plan_send: (a: number, b: number, c: number, d: number, e: bigint, f: bigint) => [number, number, number];
    readonly reconcile_wallet_store: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly recover_wallet_from_chain: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly respond_to_slate: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly restore_keystore_from_mnemonic: (a: number, b: number) => [number, number, number, number];
    readonly reveal_stake_blinding_hex: (a: number, b: number, c: number, d: number, e: bigint) => [number, number, number, number];
    readonly sign_allowlist_publish: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: bigint) => [number, number, number, number];
    readonly sign_collection_mint_approval: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => [number, number, number, number];
    readonly sign_identity_message: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly sweep_validator_rewards: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: bigint) => [number, number, number];
    readonly tx_kernel_excess_hex: (a: number, b: number) => [number, number, number, number];
    readonly verify_identity_signature: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly wallet_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_identity_pubkey_hex: (a: number, b: number) => [number, number, number, number];
    readonly wallet_pending_balance: (a: number, b: number) => [bigint, number, number];
    readonly wallet_store_new: () => [number, number];
    readonly __wbg_get_wasmownedoutput_index: (a: number) => number;
    readonly __wbg_get_wasmrecoveryresult_recovered_balance: (a: number) => bigint;
    readonly __wbg_get_wasmmintassetresult_change: (a: number) => number;
    readonly __wbg_get_wasmregisternameresult_change: (a: number) => number;
    readonly __wbg_get_wasmslatereservation_change: (a: number) => number;
    readonly __wbg_set_wasmownedoutput_index: (a: number, b: number) => void;
    readonly __wbg_set_wasmrecoveryresult_recovered_balance: (a: number, b: bigint) => void;
    readonly __wbg_set_wasmmintassetresult_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmregisternameresult_change: (a: number, b: number) => void;
    readonly __wbg_set_wasmslatereservation_change: (a: number, b: number) => void;
    readonly __wbg_get_wasmsendplan_dest: (a: number) => number;
    readonly __wbg_get_wasmsweepresult_dest: (a: number) => number;
    readonly __wbg_set_wasmregisternameresult_spent_commitments_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmkeystoreandmnemonic_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmkeystoreandmnemonic_mnemonic: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmmerkleproofresult_root_hex: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmmintassetresult_op_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrecoveryresult_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrecoveryresult_store_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmregisternameresult_op_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmregisternameresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrespondresult_response_slate_json: (a: number, b: number, c: number) => void;
    readonly __wbg_set_wasmrespondresult_updated_keystore_bytes: (a: number, b: number, c: number) => void;
    readonly __wbg_get_wasmrecoveryresult_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmrecoveryresult_store_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmregisternameresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmrespondresult_updated_keystore_bytes: (a: number) => [number, number];
    readonly __wbg_get_wasmmerkleproofresult_root_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmmintassetresult_op_json: (a: number) => [number, number];
    readonly __wbg_get_wasmownedoutput_commitment_hex: (a: number) => [number, number];
    readonly __wbg_get_wasmregisternameresult_op_json: (a: number) => [number, number];
    readonly __wbg_get_wasmrespondresult_response_slate_json: (a: number) => [number, number];
    readonly __wbg_set_wasmsendplan_dest: (a: number, b: number) => void;
    readonly __wbg_set_wasmsweepresult_dest: (a: number, b: number) => void;
    readonly __wbg_get_wasmregisternameresult_spent_commitments_hex: (a: number) => [number, number];
    readonly commit_slate_send: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly commit_register_name: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
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
