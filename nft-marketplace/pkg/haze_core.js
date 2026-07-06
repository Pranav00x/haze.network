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

export class WasmKeystoreAndMnemonic {
    static __wrap(ptr) {
        const obj = Object.create(WasmKeystoreAndMnemonic.prototype);
        obj.__wbg_ptr = ptr;
        WasmKeystoreAndMnemonicFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmKeystoreAndMnemonicFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmkeystoreandmnemonic_free(ptr, 0);
    }
    /**
     * @returns {Uint8Array}
     */
    get keystore_bytes() {
        const ret = wasm.__wbg_get_wasmkeystoreandmnemonic_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Only ever available right here at generation time - the keystore
     * itself never stores or re-derives it. The caller is responsible for
     * showing it to the user and requiring confirmation it's been saved.
     * @returns {string}
     */
    get mnemonic() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmkeystoreandmnemonic_mnemonic(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @param {Uint8Array} arg0
     */
    set keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmkeystoreandmnemonic_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * Only ever available right here at generation time - the keystore
     * itself never stores or re-derives it. The caller is responsible for
     * showing it to the user and requiring confirmation it's been saved.
     * @param {string} arg0
     */
    set mnemonic(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmkeystoreandmnemonic_mnemonic(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmKeystoreAndMnemonic.prototype[Symbol.dispose] = WasmKeystoreAndMnemonic.prototype.free;

export class WasmMintAssetResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmMintAssetResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmMintAssetResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmMintAssetResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmmintassetresult_free(ptr, 0);
    }
    /**
     * @returns {WasmOwnedOutput | undefined}
     */
    get change() {
        const ret = wasm.__wbg_get_wasmmintassetresult_change(this.__wbg_ptr);
        return ret === 0 ? undefined : WasmOwnedOutput.__wrap(ret);
    }
    /**
     * POST this to /v1/assets/mint.
     * @returns {string}
     */
    get op_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmmintassetresult_op_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string[]}
     */
    get spent_commitments_hex() {
        const ret = wasm.__wbg_get_wasmmintassetresult_spent_commitments_hex(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    get updated_keystore_bytes() {
        const ret = wasm.__wbg_get_wasmmintassetresult_updated_keystore_bytes(this.__wbg_ptr);
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
        wasm.__wbg_set_wasmmintassetresult_change(this.__wbg_ptr, ptr0);
    }
    /**
     * POST this to /v1/assets/mint.
     * @param {string} arg0
     */
    set op_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmmintassetresult_op_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {string[]} arg0
     */
    set spent_commitments_hex(arg0) {
        const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmmintassetresult_spent_commitments_hex(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmmintassetresult_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmMintAssetResult.prototype[Symbol.dispose] = WasmMintAssetResult.prototype.free;

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

export class WasmRecoveryResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmRecoveryResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmRecoveryResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmRecoveryResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmrecoveryresult_free(ptr, 0);
    }
    /**
     * @returns {Uint8Array}
     */
    get keystore_bytes() {
        const ret = wasm.__wbg_get_wasmrecoveryresult_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {bigint}
     */
    get recovered_balance() {
        const ret = wasm.__wbg_get_wasmrecoveryresult_recovered_balance(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * @returns {number}
     */
    get recovered_count() {
        const ret = wasm.__wbg_get_wasmrecoveryresult_recovered_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {Uint8Array}
     */
    get store_bytes() {
        const ret = wasm.__wbg_get_wasmrecoveryresult_store_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {Uint8Array} arg0
     */
    set keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmrecoveryresult_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {bigint} arg0
     */
    set recovered_balance(arg0) {
        wasm.__wbg_set_wasmrecoveryresult_recovered_balance(this.__wbg_ptr, arg0);
    }
    /**
     * @param {number} arg0
     */
    set recovered_count(arg0) {
        wasm.__wbg_set_wasmrecoveryresult_recovered_count(this.__wbg_ptr, arg0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set store_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmrecoveryresult_store_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmRecoveryResult.prototype[Symbol.dispose] = WasmRecoveryResult.prototype.free;

export class WasmRegisterNameResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmRegisterNameResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmRegisterNameResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmRegisterNameResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmregisternameresult_free(ptr, 0);
    }
    /**
     * @returns {WasmOwnedOutput | undefined}
     */
    get change() {
        const ret = wasm.__wbg_get_wasmregisternameresult_change(this.__wbg_ptr);
        return ret === 0 ? undefined : WasmOwnedOutput.__wrap(ret);
    }
    /**
     * POST this to /v1/names/register.
     * @returns {string}
     */
    get op_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmregisternameresult_op_json(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string[]}
     */
    get spent_commitments_hex() {
        const ret = wasm.__wbg_get_wasmregisternameresult_spent_commitments_hex(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    get updated_keystore_bytes() {
        const ret = wasm.__wbg_get_wasmregisternameresult_updated_keystore_bytes(this.__wbg_ptr);
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
        wasm.__wbg_set_wasmregisternameresult_change(this.__wbg_ptr, ptr0);
    }
    /**
     * POST this to /v1/names/register.
     * @param {string} arg0
     */
    set op_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmregisternameresult_op_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {string[]} arg0
     */
    set spent_commitments_hex(arg0) {
        const ptr0 = passArrayJsValueToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmregisternameresult_spent_commitments_hex(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmregisternameresult_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmRegisterNameResult.prototype[Symbol.dispose] = WasmRegisterNameResult.prototype.free;

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

export class WasmSweepResult {
    static __wrap(ptr) {
        const obj = Object.create(WasmSweepResult.prototype);
        obj.__wbg_ptr = ptr;
        WasmSweepResultFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmSweepResultFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmsweepresult_free(ptr, 0);
    }
    /**
     * Add this to the wallet's own store as Pending on success (reuse
     * commit_send with an empty spent_commitments_hex and no change - the
     * swept reward inputs were never part of this wallet's own store to
     * begin with, only the destination output is new).
     * @returns {WasmOwnedOutput}
     */
    get dest() {
        const ret = wasm.__wbg_get_wasmsweepresult_dest(this.__wbg_ptr);
        return WasmOwnedOutput.__wrap(ret);
    }
    /**
     * @returns {number}
     */
    get swept_count() {
        const ret = wasm.__wbg_get_wasmsweepresult_swept_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {bigint}
     */
    get swept_total() {
        const ret = wasm.__wbg_get_wasmsweepresult_swept_total(this.__wbg_ptr);
        return BigInt.asUintN(64, ret);
    }
    /**
     * POST this to /v1/transactions.
     * @returns {string}
     */
    get transaction_json() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.__wbg_get_wasmsweepresult_transaction_json(this.__wbg_ptr);
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
        const ret = wasm.__wbg_get_wasmsweepresult_updated_keystore_bytes(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Add this to the wallet's own store as Pending on success (reuse
     * commit_send with an empty spent_commitments_hex and no change - the
     * swept reward inputs were never part of this wallet's own store to
     * begin with, only the destination output is new).
     * @param {WasmOwnedOutput} arg0
     */
    set dest(arg0) {
        _assertClass(arg0, WasmOwnedOutput);
        var ptr0 = arg0.__destroy_into_raw();
        wasm.__wbg_set_wasmsweepresult_dest(this.__wbg_ptr, ptr0);
    }
    /**
     * @param {number} arg0
     */
    set swept_count(arg0) {
        wasm.__wbg_set_wasmsweepresult_swept_count(this.__wbg_ptr, arg0);
    }
    /**
     * @param {bigint} arg0
     */
    set swept_total(arg0) {
        wasm.__wbg_set_wasmsweepresult_swept_total(this.__wbg_ptr, arg0);
    }
    /**
     * POST this to /v1/transactions.
     * @param {string} arg0
     */
    set transaction_json(arg0) {
        const ptr0 = passStringToWasm0(arg0, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmsweepresult_transaction_json(this.__wbg_ptr, ptr0, len0);
    }
    /**
     * @param {Uint8Array} arg0
     */
    set updated_keystore_bytes(arg0) {
        const ptr0 = passArray8ToWasm0(arg0, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        wasm.__wbg_set_wasmsweepresult_updated_keystore_bytes(this.__wbg_ptr, ptr0, len0);
    }
}
if (Symbol.dispose) WasmSweepResult.prototype[Symbol.dispose] = WasmSweepResult.prototype.free;

/**
 * Builds a signed cancellation for a listing this wallet previously
 * created - see POST /v1/marketplace/cancel.
 * @param {Uint8Array} keystore_bytes
 * @param {string} asset_id
 * @returns {string}
 */
export function build_cancel_listing_request(keystore_bytes, asset_id) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(asset_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.build_cancel_listing_request(ptr0, len0, ptr1, len1);
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
 * Builds a signed marketplace Listing (see core::marketplace) advertising
 * an asset this wallet owns for sale at `price`, signed with this wallet's
 * identity key - the same key the asset's owner_pubkey on-chain is
 * expected to match, checked server-side at POST /v1/marketplace/list.
 * @param {Uint8Array} keystore_bytes
 * @param {string} asset_id
 * @param {bigint} price
 * @param {bigint} listed_at
 * @returns {string}
 */
export function build_create_listing_request(keystore_bytes, asset_id, price, listed_at) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(asset_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.build_create_listing_request(ptr0, len0, ptr1, len1, price, listed_at);
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
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {string} asset_id
 * @param {string} metadata
 * @param {bigint} fee
 * @returns {WasmMintAssetResult}
 */
export function build_mint_asset_request(keystore_bytes, store_bytes, asset_id, metadata, fee) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(asset_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passStringToWasm0(metadata, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len3 = WASM_VECTOR_LEN;
    const ret = wasm.build_mint_asset_request(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, fee);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmMintAssetResult.__wrap(ret[0]);
}

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
 * @param {Uint8Array} keystore_bytes
 * @param {Uint8Array} store_bytes
 * @param {string} name
 * @param {bigint} fee
 * @returns {WasmRegisterNameResult}
 */
export function build_register_name_request(keystore_bytes, store_bytes, name, fee) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.build_register_name_request(ptr0, len0, ptr1, len1, ptr2, len2, fee);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmRegisterNameResult.__wrap(ret[0]);
}

/**
 * Builds a sponsored registration request body for POST
 * /v1/names/register-sponsored - unlike build_register_name_request, this
 * needs no store/UTXOs/coin-selection at all, since the node's own faucet
 * reserve covers the flat registration fee (see FaucetState::
 * build_sponsored_fee_payment on the server side). This is what lets a
 * brand-new wallet register a name before it has ever received any funds.
 * @param {Uint8Array} keystore_bytes
 * @param {string} name
 * @returns {string}
 */
export function build_sponsored_register_name_request(keystore_bytes, name) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.build_sponsored_register_name_request(ptr0, len0, ptr1, len1);
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
 * @param {Uint8Array} keystore_bytes
 * @param {string} asset_id
 * @param {string} new_owner_pubkey_hex
 * @param {string | null} [required_kernel_excess_hex]
 * @returns {string}
 */
export function build_transfer_asset_request(keystore_bytes, asset_id, new_owner_pubkey_hex, required_kernel_excess_hex) {
    let deferred6_0;
    let deferred6_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(asset_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(new_owner_pubkey_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        var ptr3 = isLikeNone(required_kernel_excess_hex) ? 0 : passStringToWasm0(required_kernel_excess_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len3 = WASM_VECTOR_LEN;
        const ret = wasm.build_transfer_asset_request(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        var ptr5 = ret[0];
        var len5 = ret[1];
        if (ret[3]) {
            ptr5 = 0; len5 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred6_0 = ptr5;
        deferred6_1 = len5;
        return getStringFromWasm0(ptr5, len5);
    } finally {
        wasm.__wbindgen_free(deferred6_0, deferred6_1, 1);
    }
}

/**
 * Builds a TransferNameOp handing a name this wallet currently owns to a
 * new owner/resolution target, signed with this wallet's identity key. No
 * fee, no UTXO involved - the server rejects it if the signature doesn't
 * actually match the name's current on-chain owner. `new_resolves_to_hex`
 * is usually the same as `new_owner_pubkey_hex`, but kept separate to match
 * the underlying protocol (they're allowed to differ).
 * @param {Uint8Array} keystore_bytes
 * @param {string} name
 * @param {string} new_owner_pubkey_hex
 * @param {string} new_resolves_to_hex
 * @returns {string}
 */
export function build_transfer_name_request(keystore_bytes, name, new_owner_pubkey_hex, new_resolves_to_hex) {
    let deferred6_0;
    let deferred6_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(new_owner_pubkey_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(new_resolves_to_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.build_transfer_name_request(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        var ptr5 = ret[0];
        var len5 = ret[1];
        if (ret[3]) {
            ptr5 = 0; len5 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred6_0 = ptr5;
        deferred6_1 = len5;
        return getStringFromWasm0(ptr5, len5);
    } finally {
        wasm.__wbindgen_free(deferred6_0, deferred6_1, 1);
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
 * Applies a previously-built asset mint's effects (spent inputs, optional
 * change) to the store. Must only be called after the mint was successfully
 * queued via POST /v1/assets/mint. Identical bookkeeping to
 * commit_register_name - kept as its own function so the JS side has a
 * clearly-scoped call per feature.
 * @param {Uint8Array} store_bytes
 * @param {string[]} spent_commitments_hex
 * @param {WasmOwnedOutput | null} [change]
 * @returns {Uint8Array}
 */
export function commit_mint_asset(store_bytes, spent_commitments_hex, change) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArrayJsValueToWasm0(spent_commitments_hex, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    let ptr2 = 0;
    if (!isLikeNone(change)) {
        _assertClass(change, WasmOwnedOutput);
        ptr2 = change.__destroy_into_raw();
    }
    const ret = wasm.commit_mint_asset(ptr0, len0, ptr1, len1, ptr2);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v4 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v4;
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
 * Applies a previously-built name registration's effects (spent inputs,
 * optional change) to the store. Must only be called after the registration
 * was successfully queued via POST /v1/names/register.
 * @param {Uint8Array} store_bytes
 * @param {string[]} spent_commitments_hex
 * @param {WasmOwnedOutput | null} [change]
 * @returns {Uint8Array}
 */
export function commit_register_name(store_bytes, spent_commitments_hex, change) {
    const ptr0 = passArray8ToWasm0(store_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArrayJsValueToWasm0(spent_commitments_hex, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    let ptr2 = 0;
    if (!isLikeNone(change)) {
        _assertClass(change, WasmOwnedOutput);
        ptr2 = change.__destroy_into_raw();
    }
    const ret = wasm.commit_register_name(ptr0, len0, ptr1, len1, ptr2);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v4 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v4;
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
 * Generates a fresh keystore backed by a real 12-word BIP39 mnemonic, so it
 * can be recovered later via restore_keystore_from_mnemonic().
 * @returns {WasmKeystoreAndMnemonic}
 */
export function generate_keystore_with_mnemonic() {
    const ret = wasm.generate_keystore_with_mnemonic();
    return WasmKeystoreAndMnemonic.__wrap(ret);
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
 * @param {Uint8Array} keystore_bytes
 * @param {string} scan_entries_json
 * @param {string[]} chain_utxo_commitments_hex
 * @returns {WasmRecoveryResult}
 */
export function recover_wallet_from_chain(keystore_bytes, scan_entries_json, chain_utxo_commitments_hex) {
    const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(scan_entries_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArrayJsValueToWasm0(chain_utxo_commitments_hex, wasm.__wbindgen_malloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.recover_wallet_from_chain(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmRecoveryResult.__wrap(ret[0]);
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
 * Reconstructs a keystore from a previously-generated BIP39 phrase.
 * @param {string} phrase
 * @returns {Uint8Array}
 */
export function restore_keystore_from_mnemonic(phrase) {
    const ptr0 = passStringToWasm0(phrase, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.restore_keystore_from_mnemonic(ptr0, len0);
    if (ret[3]) {
        throw takeFromExternrefTable0(ret[2]);
    }
    var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    return v2;
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
 * Signs an arbitrary UTF-8 message with this wallet's identity key - used
 * by the standalone marketplace site's "connect wallet" handoff (see
 * haze-marketplace-web) to let the wallet prove control of its identity
 * pubkey over a marketplace-issued nonce, without the marketplace site ever
 * touching the wallet's keys directly.
 * @param {Uint8Array} keystore_bytes
 * @param {string} message
 * @returns {string}
 */
export function sign_identity_message(keystore_bytes, message) {
    let deferred4_0;
    let deferred4_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(message, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.sign_identity_message(ptr0, len0, ptr1, len1);
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
 * Finds every still-unspent block reward this validator has ever earned
 * (see wallet::note::coinbase_blinding/coinbase_note_key and
 * core::proposer, which now derives coinbase blindings from the staking
 * secret instead of a discarded random one) and sweeps all of them into a
 * single new output in this wallet's own keystore - turning "provably mine
 * but nowhere to spend it from" into an ordinary, self-owned, spendable
 * balance. `stake_key_hex` is the same secret reveal_stake_blinding_hex
 * already exposes for running a validator node with. Errors if nothing
 * unswept is found, or if the total found doesn't even cover `fee`.
 * @param {string} stake_key_hex
 * @param {string} scan_entries_json
 * @param {string[]} chain_utxo_commitments_hex
 * @param {Uint8Array} keystore_bytes
 * @param {bigint} fee
 * @returns {WasmSweepResult}
 */
export function sweep_validator_rewards(stake_key_hex, scan_entries_json, chain_utxo_commitments_hex, keystore_bytes, fee) {
    const ptr0 = passStringToWasm0(stake_key_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(scan_entries_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passArrayJsValueToWasm0(chain_utxo_commitments_hex, wasm.__wbindgen_malloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
    const len3 = WASM_VECTOR_LEN;
    const ret = wasm.sweep_validator_rewards(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, fee);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WasmSweepResult.__wrap(ret[0]);
}

/**
 * Extracts a finalized (but not necessarily yet broadcast) transaction's
 * kernel excess as hex - used by a marketplace buyer to learn the exact
 * value to send the seller in a "want_transfer" inbox message, so the
 * seller can build a TransferAssetOp conditioned on this specific payment
 * (see build_transfer_asset_request's required_kernel_excess_hex). Every
 * Haze transaction has exactly one kernel by construction (see
 * wallet::slate::finalize_slate/wallet::planner::plan_send), so this
 * always reads kernels[0].
 * @param {string} transaction_json
 * @returns {string}
 */
export function tx_kernel_excess_hex(transaction_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passStringToWasm0(transaction_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.tx_kernel_excess_hex(ptr0, len0);
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Verifies a signature produced by sign_identity_message - lets the
 * marketplace site check a "connect wallet" handoff's proof-of-pubkey
 * client-side, with no server round-trip and no key material involved.
 * @param {string} pubkey_hex
 * @param {string} message
 * @param {string} signature_hex
 * @returns {boolean}
 */
export function verify_identity_signature(pubkey_hex, message, signature_hex) {
    const ptr0 = passStringToWasm0(pubkey_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(message, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(signature_hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.verify_identity_signature(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] !== 0;
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
 * Derives this wallet's stable naming-registry identity pubkey (hex), so the
 * UI can show "your names resolve to this pubkey" without needing a
 * registration to already exist.
 * @param {Uint8Array} keystore_bytes
 * @returns {string}
 */
export function wallet_identity_pubkey_hex(keystore_bytes) {
    let deferred3_0;
    let deferred3_1;
    try {
        const ptr0 = passArray8ToWasm0(keystore_bytes, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wallet_identity_pubkey_hex(ptr0, len0);
        var ptr2 = ret[0];
        var len2 = ret[1];
        if (ret[3]) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(ret[2]);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
    }
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
const WasmKeystoreAndMnemonicFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmkeystoreandmnemonic_free(ptr, 1));
const WasmMintAssetResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmmintassetresult_free(ptr, 1));
const WasmOwnedOutputFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmownedoutput_free(ptr, 1));
const WasmRecoveryResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmrecoveryresult_free(ptr, 1));
const WasmRegisterNameResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmregisternameresult_free(ptr, 1));
const WasmRespondResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmrespondresult_free(ptr, 1));
const WasmSendPlanFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmsendplan_free(ptr, 1));
const WasmSweepResultFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmsweepresult_free(ptr, 1));

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
