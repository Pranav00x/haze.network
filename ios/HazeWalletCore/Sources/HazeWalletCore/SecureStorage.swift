import Foundation
import Security

/// Persists the wallet's keystore (secret) in the iOS Keychain and
/// everything else (the non-secret wallet-store ledger, node/explorer
/// URLs, claimed name, activity log) in UserDefaults - the same
/// secret/non-secret split Android's SecureStorage.kt makes between the
/// Android Keystore and its own SharedPreferences.
public final class SecureStorage {
    public static let defaultNodeUrl = "https://haze-b3l9.onrender.com"
    public static let defaultExplorerUrl = "https://haze-b3l9.onrender.com"

    private let defaults = UserDefaults.standard
    private let keychainAccount = "com.haze.wallet.keystore"

    public init() {}

    // ---------------- keystore (secret, Keychain) ----------------

    public func loadKeystoreBytes() -> Data? {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: keychainAccount,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data else { return nil }
        return data
    }

    public func saveKeystoreBytes(_ bytes: Data) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: keychainAccount,
        ]
        SecItemDelete(query as CFDictionary)
        var addQuery = query
        addQuery[kSecValueData as String] = bytes
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(addQuery as CFDictionary, nil)
    }

    private func wipeKeystoreBytes() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: keychainAccount,
        ]
        SecItemDelete(query as CFDictionary)
    }

    // ---------------- everything else (UserDefaults) ----------------

    public func loadStoreBytes() -> Data? { defaults.data(forKey: "hazeStoreBytes") }
    public func saveStoreBytes(_ bytes: Data) { defaults.set(bytes, forKey: "hazeStoreBytes") }

    public func loadNodeUrl() -> String { defaults.string(forKey: "hazeNodeUrl") ?? Self.defaultNodeUrl }
    public func saveNodeUrl(_ url: String) { defaults.set(url, forKey: "hazeNodeUrl") }

    public func loadExplorerUrl() -> String { defaults.string(forKey: "hazeExplorerUrl") ?? Self.defaultExplorerUrl }
    public func saveExplorerUrl(_ url: String) { defaults.set(url, forKey: "hazeExplorerUrl") }

    public func loadClaimedName() -> String? { defaults.string(forKey: "hazeClaimedName") }
    public func saveClaimedName(_ name: String) { defaults.set(name, forKey: "hazeClaimedName") }

    public func loadActivityLogJson() -> String { defaults.string(forKey: "hazeActivityLog") ?? "[]" }
    public func saveActivityLogJson(_ json: String) { defaults.set(json, forKey: "hazeActivityLog") }

    /// Clears everything - keystore, ledger, claimed name, activity - but
    /// deliberately leaves nodeUrl/explorerUrl alone, same as Android's
    /// lockWallet(), so re-onboarding doesn't lose those settings.
    public func wipe() {
        wipeKeystoreBytes()
        defaults.removeObject(forKey: "hazeStoreBytes")
        defaults.removeObject(forKey: "hazeClaimedName")
        defaults.removeObject(forKey: "hazeActivityLog")
    }
}
