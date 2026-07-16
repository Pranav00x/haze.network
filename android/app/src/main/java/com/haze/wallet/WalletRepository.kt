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
    val explorerUrl: String = SecureStorage.DEFAULT_EXPLORER_URL,
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
            explorerUrl = storage.loadExplorerUrl(),
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

    fun setExplorerUrl(url: String) {
        storage.saveExplorerUrl(url)
        _state.update { it.copy(explorerUrl = url) }
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
        _state.value = WalletUiState(nodeUrl = storage.loadNodeUrl(), explorerUrl = storage.loadExplorerUrl())
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

    // ---------------- seed rotation ----------------
    // Pure passthrough (no repository state touched) - the UI holds the
    // result locally while the user confirms they've saved the new phrase,
    // then hands the keystore bytes back via executeSeedRotation below.
    // Kept here rather than called directly from MainActivity so the UI
    // layer still never touches uniffi.haze_core itself.
    fun generateRotationCandidate(): FfiKeystoreAndMnemonic = generateKeystoreWithMnemonic()

    // There's no "account" to re-key here - owning a coin means knowing its
    // blinding factor, derived from the seed that sealed it. "Replacing" a
    // seed is therefore a real on-chain sweep: spend everything the old
    // seed owns into fresh outputs owned by a brand-new seed, in one
    // transaction. The UI generates the new keystore+mnemonic first (via
    // generateKeystoreWithMnemonic(), shown so the user can confirm they've
    // saved it) before calling this with those bytes - kept as two steps
    // so the sweep only happens after that confirmation.
    suspend fun executeSeedRotation(newKeystoreBytes: ByteArray): String? = withContext(Dispatchers.IO) {
        try {
            val fee = currentFeeEstimate()
            val result = rotateSeedTransaction(requireKeystore(), storeBytes, newKeystoreBytes, fee.toULong())
            api.submitTransaction(result.transactionJson)

            // Best-effort: hand the claimed name (if any) to the new
            // identity too, so it doesn't keep pointing at a wallet that's
            // about to be abandoned. Doesn't block rotation itself - funds
            // have already moved by this point regardless.
            val myName = _state.value.claimedName
            if (myName != null) {
                try {
                    val newPubkeyHex = walletIdentityPubkeyHex(newKeystoreBytes)
                    val opJson = buildTransferNameRequest(requireKeystore(), myName, newPubkeyHex, newPubkeyHex)
                    api.transferName(opJson)
                } catch (e: Exception) {
                    // Non-fatal - see comment above.
                }
            }

            val newStoreBytes = commitSend(walletStoreNew(), emptyList(), result.dest, null)
            persistKeystore(newKeystoreBytes)
            persistStore(newStoreBytes)
            pushActivity("Rotated to a new seed phrase", "Moved balance to a new wallet")
            refreshBalance()
            null
        } catch (e: Exception) {
            e.message ?: "failed to rotate seed"
        }
    }

    private fun jsonBytesToHex(arr: JSONArray): String {
        val bytes = ByteArray(arr.length()) { i -> arr.getInt(i).toByte() }
        return bytes.joinToString("") { String.format("%02x", it) }
    }

    // ---------------- marketplace: minting ----------------

    private fun currentAssetFeeEstimate(): Long =
        try { api.feeEstimate().getLong("suggested_asset_fee") } catch (e: Exception) { 2_000L }

    suspend fun mintAsset(assetId: String, metadata: String): String? = withContext(Dispatchers.IO) {
        try {
            val fee = currentAssetFeeEstimate()
            val built = buildMintAssetRequest(requireKeystore(), storeBytes, assetId.trim(), metadata, fee.toULong())
            persistKeystore(built.updatedKeystoreBytes)
            api.mintAsset(built.opJson)
            persistStore(commitMintAsset(storeBytes, built.spentCommitmentsHex, built.change))
            pushActivity("Minted $assetId", "")
            refreshBalance()
            null
        } catch (e: Exception) {
            e.message ?: "failed to mint asset"
        }
    }

    // ---------------- marketplace: browsing / listing ----------------

    data class MarketAsset(val assetId: String, val ownerPubkeyHex: String, val metadata: String, val collectionId: String?)
    data class MarketListing(val assetId: String, val sellerPubkeyHex: String, val price: Long, val listedAt: Long)

    private fun assetFromJson(o: JSONObject): MarketAsset = MarketAsset(
        assetId = o.getString("asset_id"),
        ownerPubkeyHex = jsonBytesToHex(o.getJSONArray("owner_pubkey")),
        metadata = try {
            val bytes = o.getJSONArray("metadata")
            ByteArray(bytes.length()) { i -> bytes.getInt(i).toByte() }.toString(Charsets.UTF_8)
        } catch (e: Exception) { "" },
        collectionId = if (o.isNull("collection_id")) null else o.optString("collection_id", null),
    )

    private fun listingFromJson(o: JSONObject): MarketListing = MarketListing(
        assetId = o.getString("asset_id"),
        sellerPubkeyHex = jsonBytesToHex(o.getJSONArray("seller_pubkey")),
        price = o.getLong("price"),
        listedAt = o.getLong("listed_at"),
    )

    suspend fun myAssets(): List<MarketAsset> = withContext(Dispatchers.IO) {
        val myPubkeyHex = walletIdentityPubkeyHex(requireKeystore())
        val arr = api.listAssets(200)
        (0 until arr.length()).map { assetFromJson(arr.getJSONObject(it)) }.filter { it.ownerPubkeyHex == myPubkeyHex }
    }

    suspend fun browseListings(): List<MarketListing> = withContext(Dispatchers.IO) {
        val arr = api.listListings(200)
        (0 until arr.length()).map { listingFromJson(arr.getJSONObject(it)) }
    }

    suspend fun listAssetForSale(assetId: String, price: Long): String? = withContext(Dispatchers.IO) {
        try {
            val listingJson = buildCreateListingRequest(requireKeystore(), assetId.trim(), price.toULong(), (System.currentTimeMillis() / 1000L).toULong())
            api.createListing(listingJson)
            pushActivity("Listed $assetId for $price", "")
            null
        } catch (e: Exception) {
            e.message ?: "failed to list asset"
        }
    }

    suspend fun cancelAssetListing(assetId: String): String? = withContext(Dispatchers.IO) {
        try {
            val cancelJson = buildCancelListingRequest(requireKeystore(), assetId.trim())
            api.cancelListing(cancelJson)
            pushActivity("Cancelled listing for $assetId", "")
            null
        } catch (e: Exception) {
            e.message ?: "failed to cancel listing"
        }
    }

    // ---------------- marketplace: buying (trustless handshake) ----------------
    //
    // Mirrors the web wallet's buy flow exactly (see haze-wallet-web's
    // pollInbox "response"/"want_transfer"/"signed_transfer" handling): pay
    // the seller (and, if this asset came from a royalty-charging
    // collection, the creator too - a second, independent payment) via the
    // two-party slate protocol, wait for both to accept, only THEN ask the
    // seller for a conditional TransferAssetOp (bound to the exact kernel
    // excess of the payment(s) that just landed), and only broadcast the
    // payment(s) once that signed transfer is in hand. The transfer is
    // inert until its required kernel(s) exist on-chain, so broadcasting
    // payment-then-transfer in that order carries no risk of the seller
    // taking the money and never delivering.
    suspend fun buyListing(listing: MarketListing): String? = withContext(Dispatchers.IO) {
        try {
            val myPubkeyHex = walletIdentityPubkeyHex(requireKeystore())
            val fee = currentFeeEstimate()

            var royaltyPubkeyHex: String? = null
            var royaltyAmount = 0L
            try {
                val asset = api.getAsset(listing.assetId)
                val collectionId = asset?.let { if (it.isNull("collection_id")) null else it.optString("collection_id", null) }
                if (collectionId != null) {
                    val collection = api.getCollection(collectionId)
                    val royaltyBps = collection?.optInt("royalty_bps", 0) ?: 0
                    if (royaltyBps > 0) {
                        royaltyPubkeyHex = jsonBytesToHex(collection!!.getJSONArray("creator_pubkey"))
                        royaltyAmount = (listing.price * royaltyBps) / 10000L
                    }
                }
            } catch (e: Exception) { /* asset/collection lookup failing just means no royalty applies */ }
            val hasRoyaltyLeg = royaltyPubkeyHex != null && royaltyAmount > 0L
            val sellerAmount = listing.price - royaltyAmount

            val sellerSlate = createSendSlate(requireKeystore(), storeBytes, sellerAmount.toULong(), fee.toULong())
            persistKeystore(sellerSlate.updatedKeystoreBytes)
            api.postInbox(listing.sellerPubkeyHex, myPubkeyHex, "request", sellerSlate.slateJson)

            var royaltyPendingSlateBytes: ByteArray? = null
            if (hasRoyaltyLeg) {
                // Eagerly reserve the seller slate's inputs before building
                // a second, independent slate off the same store - otherwise
                // the royalty slate could pick the exact same UTXO.
                val reservation = pendingSlateReservation(sellerSlate.pendingSlateBytes)
                persistStore(commitSlateSend(storeBytes, reservation.spentCommitmentsHex, reservation.change))
                val royaltySlate = createSendSlate(requireKeystore(), storeBytes, royaltyAmount.toULong(), fee.toULong())
                persistKeystore(royaltySlate.updatedKeystoreBytes)
                api.postInbox(royaltyPubkeyHex!!, myPubkeyHex, "request", royaltySlate.slateJson)
                royaltyPendingSlateBytes = royaltySlate.pendingSlateBytes
            }

            var sellerDone = false
            var royaltyDone = !hasRoyaltyLeg
            var finalizedTxJson: String? = null
            var spentCommitmentsHex: List<String> = emptyList()
            var change: FfiOwnedOutput? = null
            var kernelExcessHex: String? = null
            var finalizedRoyaltyTxJson: String? = null
            var royaltySpentCommitmentsHex: List<String> = emptyList()
            var royaltyChange: FfiOwnedOutput? = null
            var royaltyKernelExcessHex: String? = null

            // Phase 1: wait for the seller's (and, if present, the royalty
            // creator's) response to our payment slate(s).
            repeat(20) {
                if (sellerDone && royaltyDone) return@repeat
                val messages = api.getInbox(myPubkeyHex)
                for (i in 0 until messages.length()) {
                    val msg = messages.getJSONObject(i)
                    val from = msg.getString("from_pubkey_hex")
                    if (msg.getString("kind") == "response" && from == listing.sellerPubkeyHex && !sellerDone) {
                        val finalized = finalizeSlate(sellerSlate.pendingSlateBytes, msg.getString("payload_json"))
                        finalizedTxJson = finalized.transactionJson
                        spentCommitmentsHex = finalized.spentCommitmentsHex
                        change = finalized.change
                        kernelExcessHex = txKernelExcessHex(finalized.transactionJson)
                        sellerDone = true
                    } else if (hasRoyaltyLeg && msg.getString("kind") == "response" && from == royaltyPubkeyHex && !royaltyDone) {
                        val finalized = finalizeSlate(royaltyPendingSlateBytes!!, msg.getString("payload_json"))
                        finalizedRoyaltyTxJson = finalized.transactionJson
                        royaltySpentCommitmentsHex = finalized.spentCommitmentsHex
                        royaltyChange = finalized.change
                        royaltyKernelExcessHex = txKernelExcessHex(finalized.transactionJson)
                        royaltyDone = true
                    }
                }
                if (sellerDone && royaltyDone) return@repeat
                kotlinx.coroutines.delay(2000)
            }
            if (!sellerDone || !royaltyDone) return@withContext "seller hasn't accepted the offer yet - try again shortly"

            // Phase 2: ask the seller to sign the conditional transfer, now
            // that both kernel excesses are known.
            api.postInbox(
                listing.sellerPubkeyHex, myPubkeyHex, "want_transfer",
                JSONObject()
                    .put("asset_id", listing.assetId)
                    .put("buyer_pubkey_hex", myPubkeyHex)
                    .put("kernel_excess_hex", kernelExcessHex)
                    .put("royalty_kernel_excess_hex", royaltyKernelExcessHex)
                    .toString(),
            )

            var opJson: String? = null
            repeat(20) {
                val messages = api.getInbox(myPubkeyHex)
                for (i in 0 until messages.length()) {
                    val msg = messages.getJSONObject(i)
                    if (msg.getString("kind") == "signed_transfer" && msg.getString("from_pubkey_hex") == listing.sellerPubkeyHex) {
                        opJson = JSONObject(msg.getString("payload_json")).getString("op_json")
                    }
                }
                if (opJson != null) return@repeat
                kotlinx.coroutines.delay(2000)
            }
            val signedOpJson = opJson
                ?: return@withContext "payment accepted, but the seller hasn't signed the transfer yet - nothing was broadcast, safe to try buying again"

            // Phase 3: broadcast payment(s) first (inert until then), then
            // the now-valid conditional transfer.
            api.submitTransaction(finalizedTxJson!!)
            persistStore(commitSlateSend(storeBytes, spentCommitmentsHex, change))
            if (hasRoyaltyLeg) {
                api.submitTransaction(finalizedRoyaltyTxJson!!)
                persistStore(commitSlateSend(storeBytes, royaltySpentCommitmentsHex, royaltyChange))
            }
            api.transferAsset(signedOpJson)
            pushActivity("Bought ${listing.assetId}", "for ${listing.price}")
            refreshBalance()
            null
        } catch (e: Exception) {
            e.message ?: "failed to buy asset"
        }
    }

    // Seller-side auto-response to a buyer's "want_transfer" - signing costs
    // nothing, since the resulting TransferAssetOp is conditioned on a
    // kernel excess that only becomes real once the buyer's payment lands
    // on-chain (see core::assets::TransferAssetOp::required_kernel_excess).
    // Only meant to be called while a screen showing "My Listings" is
    // visible (this app has no background service) - mirrors the web
    // wallet's tab-must-be-open constraint exactly.
    suspend fun pollAndRespondAsSeller(): Int = withContext(Dispatchers.IO) {
        val myPubkeyHex = walletIdentityPubkeyHex(requireKeystore())
        val messages = api.getInbox(myPubkeyHex)
        var handled = 0
        for (i in 0 until messages.length()) {
            val msg = messages.getJSONObject(i)
            if (msg.getString("kind") != "want_transfer") continue
            val req = JSONObject(msg.getString("payload_json"))
            val opJson = buildTransferAssetRequest(
                requireKeystore(),
                req.getString("asset_id"),
                req.getString("buyer_pubkey_hex"),
                req.optString("kernel_excess_hex", null),
                if (req.isNull("royalty_kernel_excess_hex")) null else req.optString("royalty_kernel_excess_hex", null),
            )
            api.postInbox(req.getString("buyer_pubkey_hex"), myPubkeyHex, "signed_transfer", JSONObject().put("op_json", opJson).toString())
            handled++
        }
        handled
    }
}
