package com.haze.wallet

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import uniffi.haze_core.*

data class ActivityEntry(val title: String, val detail: String, val timestampMillis: Long)

data class WalletUiState(
    val hasWallet: Boolean = false,
    val confirmedBalance: Long = 0,
    val pendingBalance: Long = 0,
    val claimedName: String? = null,
    val nodeUrl: String = SecureStorage.DEFAULT_NODE_URL,
    val activity: List<ActivityEntry> = emptyList(),
    val busy: Boolean = false,
    val message: String? = null,
)

/**
 * The single source of truth for wallet state on this device - owns the
 * in-memory keystore/store bytes, persists them via SecureStorage after
 * every mutation, and is the only place that calls into the Rust core
 * (via the generated UniFFI bindings) or the node's HTTP API. UI screens
 * only ever read `state` and call these suspend functions; none of them
 * touch uniffi.haze_core or NodeApi directly.
 */
class WalletRepository(private val storage: SecureStorage) {
    private val api = NodeApi(storage.loadNodeUrl())

    private var keystoreBytes: ByteArray? = storage.loadKeystoreBytes()
    private var storeBytes: ByteArray = storage.loadStoreBytes() ?: walletStoreNew()

    private val _state = MutableStateFlow(
        WalletUiState(
            hasWallet = keystoreBytes != null,
            claimedName = storage.loadClaimedName(),
            nodeUrl = storage.loadNodeUrl(),
            activity = readActivity(),
        )
    )
    val state: StateFlow<WalletUiState> = _state.asStateFlow()

    private fun readActivity(): List<ActivityEntry> {
        val arr = JSONArray(storage.loadActivityLogJson())
        return (0 until arr.length()).map {
            val o = arr.getJSONObject(it)
            ActivityEntry(o.getString("title"), o.getString("detail"), o.getLong("ts"))
        }
    }

    private fun pushActivity(title: String, detail: String) {
        val updated = listOf(ActivityEntry(title, detail, System.currentTimeMillis())) + _state.value.activity
        val trimmed = updated.take(50)
        val arr = JSONArray()
        trimmed.forEach { e ->
            arr.put(JSONObject().put("title", e.title).put("detail", e.detail).put("ts", e.timestampMillis))
        }
        storage.saveActivityLogJson(arr.toString())
        _state.update { it.copy(activity = trimmed) }
    }

    private fun MutableStateFlow<WalletUiState>.update(fn: (WalletUiState) -> WalletUiState) {
        value = fn(value)
    }

    private fun requireKeystore(): ByteArray = keystoreBytes ?: error("no wallet created yet")

    private fun persistKeystore(bytes: ByteArray) {
        keystoreBytes = bytes
        storage.saveKeystoreBytes(bytes)
    }

    private fun persistStore(bytes: ByteArray) {
        storeBytes = bytes
        storage.saveStoreBytes(bytes)
    }

    fun setNodeUrl(url: String) {
        api.setBaseUrl(url)
        storage.saveNodeUrl(url)
        _state.update { it.copy(nodeUrl = url) }
    }

    // ---------------- wallet creation / restore ----------------

    suspend fun createWallet(): String = withContext(Dispatchers.Default) {
        val result = generateKeystoreWithMnemonic()
        persistKeystore(result.keystoreBytes)
        persistStore(walletStoreNew())
        _state.update { it.copy(hasWallet = true) }
        result.mnemonic
    }

    suspend fun restoreWallet(mnemonic: String) = withContext(Dispatchers.IO) {
        val kb = restoreKeystoreFromMnemonic(mnemonic.trim())
        persistKeystore(kb)

        // Recover balance by scanning every on-chain output's recoverable
        // note against this keystore's note key (see core note-recovery
        // design) - a restored wallet has no local ledger of its own.
        val scanJson = api.scanOutputsJson()
        val utxos = api.utxos()
        val recovery = recoverWalletFromChain(kb, scanJson, utxos)
        persistKeystore(recovery.keystoreBytes)
        persistStore(recovery.storeBytes)
        pushActivity("Restored wallet", "recovered ${recovery.recoveredBalance} across ${recovery.recoveredCount} output(s)")
        refreshBalance()
    }

    fun lockWallet() {
        storage.wipe()
        keystoreBytes = null
        storeBytes = walletStoreNew()
        _state.value = WalletUiState(nodeUrl = storage.loadNodeUrl())
    }

    // ---------------- balance ----------------

    suspend fun refreshBalance() = withContext(Dispatchers.IO) {
        val utxos = api.utxos()
        val reconciled = reconcileWalletStore(storeBytes, utxos)
        persistStore(reconciled)
        val confirmed = walletBalance(reconciled)
        val pending = walletPendingBalance(reconciled)
        _state.update { it.copy(confirmedBalance = confirmed.toLong(), pendingBalance = pending.toLong()) }
    }

    private fun currentFeeEstimate(): Long =
        try { api.feeEstimate().getLong("suggested_fee") } catch (e: Exception) { 5L }

    private fun currentNameFeeEstimate(): Long =
        try { api.feeEstimate().getLong("suggested_name_fee") } catch (e: Exception) { 10_000L }

    // ---------------- devnet faucet ----------------

    suspend fun claimFaucet(amount: Long) = withContext(Dispatchers.IO) {
        val slateJson = api.requestFaucet(amount)
        val responded = respondToSlate(requireKeystore(), slateJson)
        persistKeystore(responded.updatedKeystoreBytes)
        api.completeFaucet(responded.responseSlateJson)
        persistStore(commitReceive(storeBytes, responded.receiverOutput))
        pushActivity("Received $amount from devnet faucet", "")
        refreshBalance()
    }

    // ---------------- self-pay ----------------

    suspend fun selfPay(amount: Long) = withContext(Dispatchers.IO) {
        val fee = currentFeeEstimate()
        val plan = planSendFfi(requireKeystore(), storeBytes, amount.toULong(), fee.toULong())
        persistKeystore(plan.updatedKeystoreBytes)
        api.submitTransaction(plan.transactionJson)
        persistStore(commitSend(storeBytes, plan.spentCommitmentsHex, plan.dest, plan.change))
        pushActivity("Self-pay $amount", "consolidated own UTXOs")
    }

    // ---------------- two-party pay-to-name ----------------

    suspend fun sendToName(name: String, amount: Long): String? = withContext(Dispatchers.IO) {
        val resolved = api.resolveName(name) ?: return@withContext "that name isn't registered"
        val ownerPubkeyHex = jsonBytesToHex(resolved.getJSONArray("resolves_to"))
        val myPubkeyHex = walletIdentityPubkeyHex(requireKeystore())

        val fee = currentFeeEstimate()
        val created = createSendSlate(requireKeystore(), storeBytes, amount.toULong(), fee.toULong())
        persistKeystore(created.updatedKeystoreBytes)

        api.postInbox(ownerPubkeyHex, myPubkeyHex, "request", created.slateJson)

        // Poll our own inbox briefly for the recipient's response - mirrors
        // the web wallet's short poll loop, since this devnet relay has no
        // push mechanism.
        var responseJson: String? = null
        repeat(15) {
            val messages = api.getInbox(myPubkeyHex)
            for (i in 0 until messages.length()) {
                val msg = messages.getJSONObject(i)
                if (msg.getString("kind") == "response") {
                    responseJson = msg.getString("payload_json")
                }
            }
            if (responseJson != null) return@repeat
            kotlinx.coroutines.delay(2000)
        }
        val response = responseJson
            ?: return@withContext "recipient hasn't accepted yet - the payment request is waiting in their inbox, try completing it again shortly"

        val finalized = finalizeSlate(created.pendingSlateBytes, response)
        api.submitTransaction(finalized.transactionJson)
        persistStore(commitSlateSend(storeBytes, finalized.spentCommitmentsHex, finalized.change))
        pushActivity("Sent $amount to $name.haze", "")
        null
    }

    // ---------------- receiving a pasted slate (manual two-party) ----------------

    suspend fun respondToPastedSlate(slateJson: String): String = withContext(Dispatchers.Default) {
        val responded = respondToSlate(requireKeystore(), slateJson)
        persistKeystore(responded.updatedKeystoreBytes)
        persistStore(commitReceive(storeBytes, responded.receiverOutput))
        pushActivity("Received ${responded.receiverOutput.value}", "via pasted slate")
        responded.responseSlateJson
    }

    // ---------------- names ----------------

    suspend fun claimName(name: String): String? = withContext(Dispatchers.IO) {
        try {
            val reqJson = buildSponsoredRegisterNameRequest(requireKeystore(), name)
            api.registerNameSponsored(reqJson)
            storage.saveClaimedName(name)
            _state.update { it.copy(claimedName = name) }
            pushActivity("Claimed $name.haze", "sponsored - free registration")
            null
        } catch (e: Exception) {
            e.message ?: "failed to claim name"
        }
    }

    suspend fun lookupName(name: String): JSONObject? = withContext(Dispatchers.IO) { api.resolveName(name) }

    suspend fun transferName(name: String, newOwnerPubkeyHex: String): String? = withContext(Dispatchers.IO) {
        try {
            val opJson = buildTransferNameRequest(requireKeystore(), name, newOwnerPubkeyHex, newOwnerPubkeyHex)
            api.transferName(opJson)
            pushActivity("Transferred $name.haze", "")
            null
        } catch (e: Exception) {
            e.message ?: "failed to transfer name"
        }
    }

    // ---------------- validator staking ----------------

    suspend fun registerAsValidator(minValue: Long): String? = withContext(Dispatchers.IO) {
        try {
            val reqJson = buildStakeRequest(requireKeystore(), storeBytes, minValue.toULong())
            api.submitStake(reqJson)
            pushActivity("Registered as validator", "")
            null
        } catch (e: Exception) {
            e.message ?: "failed to register as validator"
        }
    }

    suspend fun revealStakeKey(minValue: Long): String = withContext(Dispatchers.Default) {
        revealStakeBlindingHex(requireKeystore(), storeBytes, minValue.toULong())
    }

    suspend fun recoverValidatorRewards(stakeKeyHex: String): String? = withContext(Dispatchers.IO) {
        try {
            val scanJson = api.scanOutputsJson()
            val utxos = api.utxos()
            val fee = currentFeeEstimate()
            val swept = sweepValidatorRewards(stakeKeyHex, scanJson, utxos, requireKeystore(), fee.toULong())
            persistKeystore(swept.updatedKeystoreBytes)
            api.submitTransaction(swept.transactionJson)
            persistStore(commitSend(storeBytes, emptyList(), swept.dest, null))
            pushActivity("Recovered ${swept.sweptTotal} in validator rewards", "${swept.sweptCount} block(s)")
            refreshBalance()
            null
        } catch (e: Exception) {
            e.message ?: "failed to recover rewards"
        }
    }

    private fun jsonBytesToHex(arr: JSONArray): String {
        val bytes = ByteArray(arr.length()) { i -> arr.getInt(i).toByte() }
        return bytes.joinToString("") { String.format("%02x", it) }
    }
}
