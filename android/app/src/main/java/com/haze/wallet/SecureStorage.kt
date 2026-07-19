package com.haze.wallet

import android.content.Context
import android.content.SharedPreferences
import android.util.Base64
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Android Keystore-backed storage for the wallet's serialized bytes
 * (keystore_bytes, store_bytes) plus small app state (node URL, activity
 * log, claimed name). Unlike the web wallet - which derives an AES key from
 * a user password, since browser storage has no hardware-backed secret of
 * its own - this relies on EncryptedSharedPreferences' own Keystore-backed
 * master key, which is already gated behind the device's own lock screen.
 * That's a stronger default on Android than a user-chosen password would be,
 * so no separate password prompt is added here.
 */
class SecureStorage(context: Context) {
    private val prefs: SharedPreferences

    init {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        prefs = EncryptedSharedPreferences.create(
            context,
            "haze_wallet_secure_prefs",
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    fun hasWallet(): Boolean = prefs.contains(KEY_KEYSTORE)

    fun saveKeystoreBytes(bytes: ByteArray) {
        prefs.edit().putString(KEY_KEYSTORE, Base64.encodeToString(bytes, Base64.NO_WRAP)).apply()
    }

    fun loadKeystoreBytes(): ByteArray? =
        prefs.getString(KEY_KEYSTORE, null)?.let { Base64.decode(it, Base64.NO_WRAP) }

    fun saveStoreBytes(bytes: ByteArray) {
        prefs.edit().putString(KEY_STORE, Base64.encodeToString(bytes, Base64.NO_WRAP)).apply()
    }

    fun loadStoreBytes(): ByteArray? =
        prefs.getString(KEY_STORE, null)?.let { Base64.decode(it, Base64.NO_WRAP) }

    fun saveNodeUrl(url: String) {
        prefs.edit().putString(KEY_NODE_URL, url).apply()
    }

    fun loadNodeUrl(): String = prefs.getString(KEY_NODE_URL, null) ?: DEFAULT_NODE_URL

    fun saveExplorerUrl(url: String) {
        prefs.edit().putString(KEY_EXPLORER_URL, url).apply()
    }

    fun loadExplorerUrl(): String = prefs.getString(KEY_EXPLORER_URL, null) ?: DEFAULT_EXPLORER_URL

    fun saveClaimedName(name: String?) {
        prefs.edit().putString(KEY_NAME, name).apply()
    }

    fun loadClaimedName(): String? = prefs.getString(KEY_NAME, null)

    fun saveActivityLogJson(json: String) {
        prefs.edit().putString(KEY_ACTIVITY, json).apply()
    }

    fun loadActivityLogJson(): String = prefs.getString(KEY_ACTIVITY, null) ?: "[]"

    /** Erases everything - used by "Lock wallet" / "Reset wallet". */
    fun wipe() {
        prefs.edit().clear().apply()
    }

    companion object {
        private const val KEY_KEYSTORE = "keystore_bytes"
        private const val KEY_STORE = "store_bytes"
        private const val KEY_NODE_URL = "node_url"
        private const val KEY_EXPLORER_URL = "explorer_url"
        private const val KEY_NAME = "claimed_name"
        private const val KEY_ACTIVITY = "activity_log"
        const val DEFAULT_NODE_URL = "https://haze-b3l9.onrender.com"
        // The node serves its own embedded block explorer at its own root -
        // same live address as DEFAULT_NODE_URL, no separate deployment.
        const val DEFAULT_EXPLORER_URL = "https://haze-b3l9.onrender.com"
    }
}
