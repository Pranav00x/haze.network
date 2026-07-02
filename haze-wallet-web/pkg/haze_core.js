/* @ts-self-types="./haze_core.d.ts" */

export class WasmCreateSlateResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmCreateSlateResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmCreateSlateResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmCreateSlateResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmcreateslateresult_free(ptr, 0);
    }
    /**
     * Keep this locally - never share it. Required by `finalize_slate` later.
     * @returns {Uint8Array}
     */
    get pending_slate_bytes() {
        const ret = wasm.__wbg_get_wasmcreateslateresult_pending_slate_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Hand this to the recipient (out-of-band: chat, email, QR, etc).
     * @returns {string}
     */
    get slate_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmcreateslateresult_slate_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    get updated_keystore_bytes() {
        const ret = wasm.__wbg_get_wasmcreateslateresult_updated_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Keep this locally - never share it. Required by `finalize_slate` later.
     * @param {Uint8Array} arg0
     */
    set pending_slate_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmcreateslateresult_pending_slate_bytes(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Hand this to the recipient (out-of-band: chat, email, QR, etc).
     * @param {string} arg0
     */
    set slate_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmcreateslateresult_slate_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmcreateslateresult_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmCreateSlateResult.prototype[Symbol.dispose] = WasmCreateSlateResult.prototype.free;

export class WasmFinalizedTx {
    static __wrap(ptr) {
        const obj = Object.create(WasmFinalizedTx.prototype);
        obj.__wbg_ptr = ptr;
        WasmFinalizedTxFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmFinalizedTxFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmfinalizedtx_free(ptr, 0);
    }
    /**
     * @returns {WasmOwnedOutput | undefined}
     */
    get change() {
        const ret = wasm.__wbg_get_wasmfinalizedtx_change(this.__wbg_ptr);
        return ret === 0 ? undefined : WasmOwnedOutput.__wrap(ret);
    }
    /**
     * @returns {string[]}
     */
    get spent_commitments_hex() {
        const ret = wasm.__wbg_get_wasmfinalizedtx_spent_commitments_hex(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {string}
     */
    get transaction_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmfinalizedtx_transaction_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {WasmOwnedOutput | null} [arg0]
     */
    set change(arg0) {
        let ptr0 = 0;
        if (!isLikeNone(arg0)) {
            _assertClass(arg0, WasmOwnedOutput);
            ptr0 = arg0.__destroy_into_raw();
        }
        wasm.__wbg_set_wasmfinalizedtx_change(this.__wbg_ptr, ptr0);
    }
    /**
     * @param {string[]} arg0
     */
    set spent_commitments_hex(arg0) {
        const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmfinalizedtx_spent_commitments_hex(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {string} arg0
     */
    set transaction_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmfinalizedtx_transaction_json(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmFinalizedTx.prototype[Symbol.dispose] = WasmFinalizedTx.prototype.free;

export class WasmOwnedOutput {
    static __wrap(ptr) {
        const obj = Object.create(WasmOwnedOutput.prototype);
        obj.__wbg_ptr = ptr;
        WasmOwnedOutputFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmOwnedOutputFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmownedoutput_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    get commitment_hex() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmownedoutput_commitment_hex(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    get index() {
        const ret = wasm.__wbg_get_wasmownedoutput_index(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {bigint}
     */
    get value() {
        const ret = wasm.__wbg_get_wasmownedoutput_value(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @param {string} arg0
     */
    set commitment_hex(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmownedoutput_commitment_hex(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {number} arg0
     */
    set index(arg0) {
        wasm.__wbg_set_wasmownedoutput_index(this.__wbg_ptr, arg0);
    }
    /**
     * @param {bigint} arg0
     */
    set value(arg0) {
        wasm.__wbg_set_wasmownedoutput_value(this.__wbg_ptr, arg0);
    }
}
if (Symbol.dispose) WasmOwnedOutput.prototype[Symbol.dispose] = WasmOwnedOutput.prototype.free;

export class WasmRespondResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmRespondResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmRespondResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmRespondResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmrespondresult_free(ptr, 0);
    }
    /**
     * @returns {WasmOwnedOutput}
     */
    get receiver_output() {
        const ret = wasm.__wbg_get_wasmrespondresult_receiver_output(this.__wbg_ptr);
        return WasmOwnedOutput.__wrap(ret);
    }
    /**
     * Send this back to the original sender.
     * @returns {string}
     */
    get response_slate_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmrespondresult_response_slate_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    get updated_keystore_bytes() {
        const ret = wasm.__wbg_get_wasmrespondresult_updated_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {WasmOwnedOutput} arg0
     */
    set receiver_output(arg0) {
        _assertClass(arg0, WasmOwnedOutput);
        var ptr0 = arg0.__destroy_into_raw();
        wasm.__wbg_set_wasmrespondresult_receiver_output(this.__wbg_ptr, ptr0);
    }
    /**
     * Send this back to the original sender.
     * @param {string} arg0
     */
    set response_slate_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmrespondresult_response_slate_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmrespondresult_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmRespondResult.prototype[Symbol.dispose] = WasmRespondResult.prototype.free;

export class WasmSendPlan {
    static __wrap(ptr) {
        const obj = Object.create(WasmSendPlan.prototype);
        obj.__wbg_ptr = ptr;
        WasmSendPlanFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmSendPlanFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmsendplan_free(ptr, 0);
    }
    /**
     * @returns {WasmOwnedOutput | undefined}
     */
    get change() {
        const ret = wasm.__wbg_get_wasmsendplan_change(this.__wbg_ptr);
        return ret === 0 ? undefined : WasmOwnedOutput.__wrap(ret);
    }
    /**
     * @returns {WasmOwnedOutput}
     */
    get dest() {
        const ret = wasm.__wbg_get_wasmsendplan_dest(this.__wbg_ptr);
        return WasmOwnedOutput.__wrap(ret);
    }
    /**
     * @returns {string[]}
     */
    get spent_commitments_hex() {
        const ret = wasm.__wbg_get_wasmsendplan_spent_commitments_hex(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {string}
     */
    get transaction_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmsendplan_transaction_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    get updated_keystore_bytes() {
        const ret = wasm.__wbg_get_wasmsendplan_updated_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {WasmOwnedOutput | null} [arg0]
     */
    set change(arg0) {
        let ptr0 = 0;
        if (!isLikeNone(arg0)) {
            _assertClass(arg0, WasmOwnedOutput);
            ptr0 = arg0.__destroy_into_raw();
        }
        wasm.__wbg_set_wasmsendplan_change(this.__wbg_ptr, ptr0);
    }
    /**
     * @param {WasmOwnedOutput} arg0
     */
    set dest(arg0) {
        _assertClass(arg0, WasmOwnedOutput);
        var ptr0 = arg0.__destroy_into_raw();
        wasm.__wbg_set_wasmsendplan_dest(this.__wbg_ptr, ptr0);
    }
    /**
     * @param {string[]} arg0
     */
    set spent_commitments_hex(arg0) {
        const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmsendplan_spent_commitments_hex(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {string} arg0
     */
    set transaction_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmsendplan_transaction_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmsendplan_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmSendPlan.prototype[Symbol.dispose] = WasmSendPlan.prototype.free;

/**
 * Builds a POST /v1/stake request body by staking the wallet's single
 * largest confirmed output. Fails if there is no confirmed output at least
 * `min_value`. Does not touch the store - staking doesn't spend anything.
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {bigint} min_value
 * @returns {string}
 */
export function build_stake_request(keystore_bytes, store_bytes, min_value) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.build_stake_request(ptr0, len0, ptr1, len1, min_value);
        var ptr3 = ret[0];
        var len3 = ret[1];
        if (ret[3]) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Seeds the store with the well-known devnet genesis output (1,000,000,
 * blinding=42) - devnet-only convenience for funding a fresh web wallet,
 * mirrors the CLI's --claim-genesis. Only one wallet instance should do this.
 * @param {Uint8Array} store_bytes
 * @returns {Uint8Array}
 */
export function claim_genesis(store_bytes) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.claim_genesis(ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
}

/**
 * Receiver-side commit: adds the output from `respond_to_slate` to the
 * store as Pending. Optimistic (same tradeoff as the CLI) - there's no
 * callback confirming the sender actually broadcasts, so this is applied
 * right after responding rather than after on-chain confirmation.
 * @param {Uint8Array} store_bytes
 * @param {WasmOwnedOutput} output
 * @returns {Uint8Array}
 */
export function commit_receive(store_bytes, output) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    _assertClass(output, WasmOwnedOutput);
    var ptr1 = output.__destroy_into_raw();
    const ret = wasm.commit_receive(ptr0, len0, ptr1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Applies a previously-built SendPlan's effects to the wallet store. Must only be
 * called after the transaction was successfully broadcast.
 * @param {Uint8Array} store_bytes
 * @param {string[]} spent_commitments_hex
 * @param {WasmOwnedOutput} dest
 * @param {WasmOwnedOutput | null} [change]
 * @returns {Uint8Array}
 */
export function commit_send(store_bytes, spent_commitments_hex, dest, change) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArrayJsValueToWasm0(spent_commitments_hex, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    _assertClass(dest, WasmOwnedOutput);
    var ptr2 = dest.__destroy_into_raw();
    let ptr3 = 0;
    if (!isLikeNone(change)) {
        _assertClass(change, WasmOwnedOutput);
        ptr3 = change.__destroy_into_raw();
    }
    const ret = wasm.commit_send(ptr0, len0, ptr1, len1, ptr2, ptr3);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v5 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v5;
}

/**
 * Sender-side commit: applies a finalized+broadcast slate payment's effects
 * (spent inputs, optional change) to the store. Must only be called after
 * the transaction was successfully broadcast.
 * @param {Uint8Array} store_bytes
 * @param {string[]} spent_commitments_hex
 * @param {WasmOwnedOutput | null} [change]
 * @returns {Uint8Array}
 */
export function commit_slate_send(store_bytes, spent_commitments_hex, change) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArrayJsValueToWasm0(spent_commitments_hex, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    let ptr2 = 0;
    if (!isLikeNone(change)) {
        _assertClass(change, WasmOwnedOutput);
        ptr2 = change.__destroy_into_raw();
    }
    const ret = wasm.commit_slate_send(ptr0, len0, ptr1, len1, ptr2);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v4 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v4;
}

/**
 * Sender step 1: builds a slate paying a different wallet `amount`. Returns
 * the slate JSON to hand to the recipient and the private pending-slate
 * bytes to keep locally until `finalize_slate`.
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {bigint} amount
 * @param {bigint} fee
 * @returns {WasmCreateSlateResult}
 */
export function create_send_slate(keystore_bytes, store_bytes, amount, fee) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.create_send_slate(ptr0, len0, ptr1, len1, amount, fee);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmCreateSlateResult.__wrap(ret[0]);
}

/**
 * Sender step 2 (final): combines the local pending slate with the
 * recipient's response into the final Transaction. The caller must POST
 * `transaction_json` itself, then call `commit_slate_send` only on success.
 * @param {Uint8Array} pending_slate_bytes
 * @param {string} response_slate_json
 * @returns {WasmFinalizedTx}
 */
export function finalize_slate(pending_slate_bytes, response_slate_json) {
    const ptr0 = passArray8ToWasm0(pending_slate_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(response_slate_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.finalize_slate(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmFinalizedTx.__wrap(ret[0]);
}

/**
 * Generates a fresh keystore (random seed, via the browser's crypto.getRandomValues
 * through getrandom's "js" feature) and returns its serialized bytes.
 * @returns {Uint8Array}
 */
export function generate_keystore() {
    const ret = wasm.generate_keystore();
    var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v1;
}

/**
 * Builds a real, self-contained transaction from the wallet's own confirmed UTXOs.
 * Allocates new output indices in the returned keystore bytes immediately (same
 * as the desktop wallet), regardless of whether the caller goes on to broadcast
 * successfully. The caller must POST `transaction_json` itself, then call
 * `commit_send` only on a successful response.
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {bigint} amount
 * @param {bigint} fee
 * @returns {WasmSendPlan}
 */
export function plan_send(keystore_bytes, store_bytes, amount, fee) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.plan_send(ptr0, len0, ptr1, len1, amount, fee);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmSendPlan.__wrap(ret[0]);
}

/**
 * Reconciles a wallet store's local ledger against the node's current on-chain
 * UTXO set (as returned by GET /v1/utxos, hex-encoded), returning updated bytes.
 * @param {Uint8Array} store_bytes
 * @param {string[]} chain_utxo_commitments_hex
 * @returns {Uint8Array}
 */
export function reconcile_wallet_store(store_bytes, chain_utxo_commitments_hex) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArrayJsValueToWasm0(chain_utxo_commitments_hex, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.reconcile_wallet_store(ptr0, len0, ptr1, len1);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v3;
}

/**
 * Receiver step: fills in a slate received from a sender. Returns the
 * response JSON to send back, plus the output info the caller should add
 * to its own store as Pending.
 * @param {Uint8Array} keystore_bytes
 * @param {string} slate_json
 * @returns {WasmRespondResult}
 */
export function respond_to_slate(keystore_bytes, slate_json) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(slate_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.respond_to_slate(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmRespondResult.__wrap(ret[0]);
}

/**
 * Reveals the raw blinding factor (as hex) for the wallet's single largest
 * confirmed output - the private key needed to actually run a node as the
 * proposer for that staked output (`haze node --stake-key <hex>`). This is
 * sensitive: it's the spending key for that output, not just a view key.
 * Only exposed so a wallet holder can run their own validator; never sent
 * anywhere except directly into the user's own node process.
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {bigint} min_value
 * @returns {string}
 */
export function reveal_stake_blinding_hex(keystore_bytes, store_bytes, min_value) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.reveal_stake_blinding_hex(ptr0, len0, ptr1, len1, min_value);
        var ptr3 = ret[0];
        var len3 = ret[1];
        if (ret[3]) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_free(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Confirmed (safely spendable) balance.
 * @param {Uint8Array} store_bytes
 * @returns {bigint}
 */
export function wallet_balance(store_bytes) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.wallet_balance(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return BigInt.asUintN(64, ret[0]);
}

/**
 * Pending (unconfirmed) balance.
 * @param {Uint8Array} store_bytes
 * @returns {bigint}
 */
export function wallet_pending_balance(store_bytes) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.wallet_pending_balance(ptr0, len0);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return BigInt.asUintN(64, ret[0]);
}

/**
 * Creates an empty wallet store and returns its serialized bytes.
 * @returns {Uint8Array}
 */
export function wallet_store_new() {
    const ret = wasm.wallet_store_new();
    var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v1;
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_is_function_1ff95bcc5517c252: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_a27215656b807791: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_ea5e6cc2e4141dfe: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_c05833b95a3cf397: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_string_get_b0ca35b86a603356: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_a6e5c5dce5018821: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_crypto_38df2bab126b63dc: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_getRandomValues_c44a50d8cfdaebeb: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_length_1f0964f4a5e2c6d8: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_msCrypto_bd5a034af96bcba6: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_new_with_length_e6785c33c8e4cce8: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_84ea875411254db1: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_process_44c7a14e11e9f69e: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_4770620bbe4688a0: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_randomFillSync_6c25eac9869eb53c: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_require_b4edbdcf3e2a1ef0: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_static_accessor_GLOBAL_4ef717fb391d88b7: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_8d1badc68b5a74f4: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_146583524fe1469b: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_f2829a2234d7819e: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_3ed232c8a6baee09: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_versions_276b2795b1c6a219: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./haze_core_bg.js": import0,
    };
}

const WasmCreateSlateResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmcreateslateresult_free(ptr, 1));
const WasmFinalizedTxFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmfinalizedtx_free(ptr, 1));
const WasmOwnedOutputFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmownedoutput_free(ptr, 1));
const WasmRespondResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmrespondresult_free(ptr, 1));
const WasmSendPlanFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmsendplan_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('haze_core_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
