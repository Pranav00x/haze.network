package com.haze.wallet

import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Thin HTTP client for the node's JSON API - mirrors exactly what the web
 * wallet does with `fetch()`. Deliberately dumb: no retries, no caching,
 * just "make the call, hand back parsed JSON or throw." All the real logic
 * (coin selection, signing, transaction construction) happens in the Rust
 * core via WalletRepository; this class only ever moves already-built JSON
 * across the wire.
 */
class NodeApi(private var baseUrl: String) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(15, TimeUnit.SECONDS)
        .build()
    private val jsonMedia = "application/json; charset=utf-8".toMediaType()

    fun setBaseUrl(url: String) {
        baseUrl = url.trimEnd('/')
    }

    fun baseUrl(): String = baseUrl

    private fun get(path: String): String {
        val request = Request.Builder().url("$baseUrl$path").get().build()
        client.newCall(request).execute().use { resp ->
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw NodeApiException(resp.code, body)
            return body
        }
    }

    private fun post(path: String, jsonBody: String): String {
        val request = Request.Builder()
            .url("$baseUrl$path")
            .post(jsonBody.toRequestBody(jsonMedia))
            .build()
        client.newCall(request).execute().use { resp ->
            val body = resp.body?.string() ?: ""
            if (!resp.isSuccessful) throw NodeApiException(resp.code, body)
            return body
        }
    }

    /** GET /v1/utxos - every commitment currently in the UTXO set, as hex. */
    fun utxos(): List<String> {
        val arr = JSONArray(get("/v1/utxos"))
        return (0 until arr.length()).map { i ->
            // The node returns each commitment as a raw byte array; convert
            // to the same lowercase hex string the Rust side uses everywhere
            // else (Commitment::to_hex).
            val bytesArr = arr.getJSONArray(i)
            val bytes = ByteArray(bytesArr.length()) { j -> bytesArr.getInt(j).toByte() }
            bytes.joinToString("") { String.format("%02x", it) }
        }
    }

    /** GET /v1/status */
    fun status(): JSONObject = JSONObject(get("/v1/status"))

    /** GET /v1/fee-estimate */
    fun feeEstimate(): JSONObject = JSONObject(get("/v1/fee-estimate"))

    /** GET /v1/scan-outputs - returns the raw JSON array (commitment_hex/note_hex pairs). */
    fun scanOutputsJson(): String = get("/v1/scan-outputs")

    /** POST /v1/transactions */
    fun submitTransaction(transactionJson: String) {
        post("/v1/transactions", transactionJson)
    }

    /** POST /v1/stake */
    fun submitStake(stakeRequestJson: String) {
        post("/v1/stake", stakeRequestJson)
    }

    /** POST /v1/faucet -> slate_json */
    fun requestFaucet(amount: Long): String {
        val body = JSONObject().put("amount", amount).toString()
        val resp = JSONObject(post("/v1/faucet", body))
        return resp.getString("slate_json")
    }

    /** POST /v1/faucet/complete */
    fun completeFaucet(responseSlateJson: String) {
        val body = JSONObject().put("response_slate_json", responseSlateJson).toString()
        post("/v1/faucet/complete", body)
    }

    /** GET /v1/names/:name -> null if not registered */
    fun resolveName(name: String): JSONObject? = try {
        JSONObject(get("/v1/names/$name"))
    } catch (e: NodeApiException) {
        if (e.code == 404) null else throw e
    }

    /** POST /v1/names/register */
    fun registerName(opJson: String) {
        post("/v1/names/register", opJson)
    }

    /** POST /v1/names/register-sponsored */
    fun registerNameSponsored(reqJson: String) {
        post("/v1/names/register-sponsored", reqJson)
    }

    /** POST /v1/names/transfer */
    fun transferName(opJson: String) {
        post("/v1/names/transfer", opJson)
    }

    /** POST /v1/inbox/:pubkeyHex */
    fun postInbox(pubkeyHex: String, fromPubkeyHex: String, kind: String, payloadJson: String) {
        val body = JSONObject()
            .put("from_pubkey_hex", fromPubkeyHex)
            .put("kind", kind)
            .put("payload_json", payloadJson)
            .toString()
        post("/v1/inbox/$pubkeyHex", body)
    }

    /** GET /v1/inbox/:pubkeyHex -> drains and returns queued messages. */
    fun getInbox(pubkeyHex: String): JSONArray = JSONArray(get("/v1/inbox/$pubkeyHex"))
}

class NodeApiException(val code: Int, val bodyText: String) : Exception("HTTP $code: $bodyText")
