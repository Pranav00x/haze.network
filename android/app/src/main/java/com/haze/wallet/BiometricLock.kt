package com.haze.wallet

import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/**
 * Wraps BiometricPrompt (biometrics, falling back to the device's own PIN/
 * pattern/password) in a single suspend call. Gates two things: unlocking
 * the wallet on launch/resume, and revealing an existing seed phrase (seed
 * rotation, validator key reveal) - both use the device's own lock-screen
 * credential rather than a separate app password, same reasoning
 * SecureStorage.kt already documents for encryption at rest.
 */
object BiometricLock {
    private const val AUTHENTICATORS = BiometricManager.Authenticators.BIOMETRIC_STRONG or
        BiometricManager.Authenticators.DEVICE_CREDENTIAL

    /** False if the device has no usable lock-screen credential at all - in
     * that case there's nothing to gate behind, so callers should treat
     * this the same as a successful unlock rather than block the wallet
     * entirely (a phone with no lock screen is a much bigger problem than
     * this app can solve). */
    fun isAvailable(activity: FragmentActivity): Boolean {
        val result = BiometricManager.from(activity).canAuthenticate(AUTHENTICATORS)
        return result == BiometricManager.BIOMETRIC_SUCCESS
    }

    suspend fun authenticate(activity: FragmentActivity, title: String, subtitle: String): Boolean {
        if (!isAvailable(activity)) return true

        return suspendCancellableCoroutine { cont ->
            val prompt = BiometricPrompt(
                activity,
                ContextCompat.getMainExecutor(activity),
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                        if (cont.isActive) cont.resume(true)
                    }
                    override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                        if (cont.isActive) cont.resume(false)
                    }
                    override fun onAuthenticationFailed() {
                        // A single failed attempt (bad fingerprint read, etc.) -
                        // the prompt stays open for another try, don't resume yet.
                    }
                },
            )
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle(title)
                .setSubtitle(subtitle)
                .setAllowedAuthenticators(AUTHENTICATORS)
                .build()
            cont.invokeOnCancellation { prompt.cancelAuthentication() }
            prompt.authenticate(info)
        }
    }
}
