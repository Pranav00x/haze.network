import Foundation

public struct ActivityEntry: Identifiable, Codable {
    public var id: String { "\(timestampMillis)-\(title)" }
    public let title: String
    public let detail: String
    public let timestampMillis: Int64
}

public struct WalletUiState {
    public var hasWallet: Bool = false
    public var confirmedBalance: UInt64 = 0
    public var pendingBalance: UInt64 = 0
    public var claimedName: String? = nil
    public var nodeUrl: String = SecureStorage.defaultNodeUrl
    public var explorerUrl: String = SecureStorage.defaultExplorerUrl
    public var activity: [ActivityEntry] = []
}

/// The single source of truth for wallet state on this device - owns the
/// in-memory keystore/store bytes, persists them via SecureStorage after
/// every mutation, and is the only place that calls into the Rust core
/// (via the generated UniFFI Swift bindings) or the node's HTTP API.
/// SwiftUI views only ever read `state` and call these async methods -
/// mirrors WalletRepository.kt's role on Android exactly.
@MainActor
public final class WalletRepository: ObservableObject {
    private let storage = SecureStorage()
    private var api: NodeApi

    private var keystoreBytes: [UInt8]?
    private var storeBytes: [UInt8]

    @Published public private(set) var state: WalletUiState

    public init() {
        let loadedKeystore = storage.loadKeystoreBytes().map { [UInt8]($0) }
        let loadedStore = storage.loadStoreBytes().map { [UInt8]($0) } ?? walletStoreNew()
        self.keystoreBytes = loadedKeystore
        self.storeBytes = loadedStore
        self.api = NodeApi(baseUrl: storage.loadNodeUrl())
        self.state = WalletUiState(
            hasWallet: loadedKeystore != nil,
            claimedName: storage.loadClaimedName(),
            nodeUrl: storage.loadNodeUrl(),
            explorerUrl: storage.loadExplorerUrl(),
            activity: Self.readActivity(storage)
        )
    }

    private static func readActivity(_ storage: SecureStorage) -> [ActivityEntry] {
        guard let data = storage.loadActivityLogJson().data(using: .utf8),
              let entries = try? JSONDecoder().decode([ActivityEntry].self, from: data) else { return [] }
        return entries
    }

    private func pushActivity(_ title: String, _ detail: String) {
        let entry = ActivityEntry(title: title, detail: detail, timestampMillis: Int64(Date().timeIntervalSince1970 * 1000))
        let trimmed = Array(([entry] + state.activity).prefix(50))
        if let data = try? JSONEncoder().encode(trimmed), let json = String(data: data, encoding: .utf8) {
            storage.saveActivityLogJson(json)
        }
        state.activity = trimmed
    }

    private func requireKeystore() throws -> [UInt8] {
        guard let kb = keystoreBytes else { throw HazeWalletError.noWallet }
        return kb
    }

    private func persistKeystore(_ bytes: [UInt8]) {
        keystoreBytes = bytes
        storage.saveKeystoreBytes(Data(bytes))
    }

    private func persistStore(_ bytes: [UInt8]) {
        storeBytes = bytes
        storage.saveStoreBytes(Data(bytes))
    }

    public func setNodeUrl(_ url: String) async {
        await api.setBaseUrl(url)
        storage.saveNodeUrl(url)
        state.nodeUrl = url
    }

    public func setExplorerUrl(_ url: String) {
        storage.saveExplorerUrl(url)
        state.explorerUrl = url
    }

    // ---------------- wallet creation / restore ----------------

    public func createWallet() -> String {
        let result = generateKeystoreWithMnemonic()
        persistKeystore(result.keystoreBytes)
        persistStore(walletStoreNew())
        state.hasWallet = true
        return result.mnemonic
    }

    public func restoreWallet(mnemonic: String) async throws {
        let kb = try restoreKeystoreFromMnemonic(mnemonic: mnemonic.trimmingCharacters(in: .whitespacesAndNewlines))
        persistKeystore(kb)

        // Recover balance by scanning every on-chain output's recoverable
        // note against this keystore's note key - a restored wallet has
        // no local ledger of its own (mirrors WalletRepository.kt).
        let scanJson = try await api.scanOutputsJson()
        let utxos = try await api.utxos()
        let recovery = try recoverWalletFromChain(keystoreBytes: kb, scanOutputsJson: scanJson, utxosHex: utxos)
        persistKeystore(recovery.keystoreBytes)
        persistStore(recovery.storeBytes)
        state.hasWallet = true
        pushActivity("Restored wallet", "recovered \(recovery.recoveredBalance) across \(recovery.recoveredCount) output(s)")
        try await refreshBalance()
    }

    public func lockWallet() {
        storage.wipe()
        keystoreBytes = nil
        storeBytes = walletStoreNew()
        state = WalletUiState(nodeUrl: storage.loadNodeUrl(), explorerUrl: storage.loadExplorerUrl())
    }

    // ---------------- balance ----------------

    public func refreshBalance() async throws {
        let utxos = try await api.utxos()
        let reconciled = reconcileWalletStore(storeBytes: storeBytes, utxosHex: utxos)
        persistStore(reconciled)
        state.confirmedBalance = walletBalance(storeBytes: reconciled)
        state.pendingBalance = walletPendingBalance(storeBytes: reconciled)
    }

    private func currentFeeEstimate() async -> UInt64 {
        (try? await api.feeEstimate()["suggested_fee"] as? UInt64) ?? 5
    }

    // ---------------- devnet faucet ----------------

    public func claimFaucet(amount: UInt64) async throws {
        let slateJson = try await api.requestFaucet(amount: amount)
        let responded = try respondToSlate(keystoreBytes: try requireKeystore(), slateJson: slateJson)
        persistKeystore(responded.updatedKeystoreBytes)
        try await api.completeFaucet(responseSlateJson: responded.responseSlateJson)
        persistStore(commitReceive(storeBytes: storeBytes, receiverOutput: responded.receiverOutput))
        pushActivity("Received \(amount) from devnet faucet", "")
        try await refreshBalance()
    }

    // ---------------- self-pay ----------------

    public func selfPay(amount: UInt64) async throws {
        let fee = await currentFeeEstimate()
        let plan = try planSendFfi(keystoreBytes: try requireKeystore(), storeBytes: storeBytes, amount: amount, fee: fee)
        persistKeystore(plan.updatedKeystoreBytes)
        try await api.submitTransaction(plan.transactionJson)
        persistStore(commitSend(storeBytes: storeBytes, spentCommitmentsHex: plan.spentCommitmentsHex, dest: plan.dest, change: plan.change))
        pushActivity("Self-pay \(amount)", "consolidated own UTXOs")
    }

    // ---------------- two-party pay-to-name ----------------

    public func sendToName(name: String, amount: UInt64) async -> String? {
        do {
            guard let resolved = try await api.resolveName(name),
                  let resolvesTo = resolved["resolves_to"] as? [Int] else {
                return "that name isn't registered"
            }
            let ownerPubkeyHex = resolvesTo.map { String(format: "%02x", $0) }.joined()
            let myPubkeyHex = try walletIdentityPubkeyHex(keystoreBytes: try requireKeystore())

            let fee = await currentFeeEstimate()
            let created = try createSendSlate(keystoreBytes: try requireKeystore(), storeBytes: storeBytes, amount: amount, fee: fee)
            persistKeystore(created.updatedKeystoreBytes)

            try await api.postInbox(pubkeyHex: ownerPubkeyHex, fromPubkeyHex: myPubkeyHex, kind: "request", payloadJson: created.slateJson)

            // Poll our own inbox briefly for the recipient's response -
            // mirrors the web/Android wallets' short poll loop, since this
            // devnet relay has no push mechanism.
            var responseJson: String? = nil
            for _ in 0..<15 {
                let messages = try await api.getInbox(pubkeyHex: myPubkeyHex)
                for msg in messages where (msg["kind"] as? String) == "response" {
                    responseJson = msg["payload_json"] as? String
                }
                if responseJson != nil { break }
                try await Task.sleep(nanoseconds: 2_000_000_000)
            }
            guard let response = responseJson else {
                return "recipient hasn't accepted yet - the payment request is waiting in their inbox, try completing it again shortly"
            }

            let finalized = try finalizeSlate(pendingSlateBytes: created.pendingSlateBytes, responseSlateJson: response)
            try await api.submitTransaction(finalized.transactionJson)
            persistStore(commitSlateSend(storeBytes: storeBytes, spentCommitmentsHex: finalized.spentCommitmentsHex, change: finalized.change))
            pushActivity("Sent \(amount) to \(name).haze", "")
            return nil
        } catch {
            return "\(error)"
        }
    }

    // ---------------- receiving a pasted slate (manual two-party) ----------------

    public func respondToPastedSlate(_ slateJson: String) throws -> String {
        let responded = try respondToSlate(keystoreBytes: try requireKeystore(), slateJson: slateJson)
        persistKeystore(responded.updatedKeystoreBytes)
        persistStore(commitReceive(storeBytes: storeBytes, receiverOutput: responded.receiverOutput))
        pushActivity("Received \(responded.receiverOutput.value)", "via pasted slate")
        return responded.responseSlateJson
    }

    // ---------------- names ----------------

    public func claimName(_ name: String) async -> String? {
        do {
            let reqJson = try buildSponsoredRegisterNameRequest(keystoreBytes: try requireKeystore(), name: name)
            try await api.registerNameSponsored(reqJson: reqJson)
            storage.saveClaimedName(name)
            state.claimedName = name
            pushActivity("Claimed \(name).haze", "sponsored - free registration")
            return nil
        } catch {
            return "\(error)"
        }
    }

    public func lookupName(_ name: String) async -> [String: Any]? {
        try? await api.resolveName(name)
    }

    public func transferName(_ name: String, newOwnerPubkeyHex: String) async -> String? {
        do {
            let opJson = try buildTransferNameRequest(keystoreBytes: try requireKeystore(), name: name, newOwnerPubkeyHex: newOwnerPubkeyHex, resolveToPubkeyHex: newOwnerPubkeyHex)
            try await api.transferName(opJson: opJson)
            pushActivity("Transferred \(name).haze", "")
            return nil
        } catch {
            return "\(error)"
        }
    }

    // ---------------- validator staking ----------------

    public func registerAsValidator(minValue: UInt64) async -> String? {
        do {
            let reqJson = try buildStakeRequest(keystoreBytes: try requireKeystore(), storeBytes: storeBytes, minValue: minValue)
            try await api.submitStake(reqJson)
            pushActivity("Registered as validator", "")
            return nil
        } catch {
            return "\(error)"
        }
    }

    public func revealStakeKey(minValue: UInt64) throws -> String {
        try revealStakeBlindingHex(keystoreBytes: try requireKeystore(), storeBytes: storeBytes, minValue: minValue)
    }

    public func recoverValidatorRewards(stakeKeyHex: String) async -> String? {
        do {
            let scanJson = try await api.scanOutputsJson()
            let utxos = try await api.utxos()
            let fee = await currentFeeEstimate()
            let swept = try sweepValidatorRewards(stakeKeyHex: stakeKeyHex, scanOutputsJson: scanJson, utxosHex: utxos, keystoreBytes: try requireKeystore(), fee: fee)
            persistKeystore(swept.updatedKeystoreBytes)
            try await api.submitTransaction(swept.transactionJson)
            persistStore(commitSend(storeBytes: storeBytes, spentCommitmentsHex: [], dest: swept.dest, change: nil))
            pushActivity("Recovered \(swept.sweptTotal) in validator rewards", "\(swept.sweptCount) block(s)")
            try await refreshBalance()
            return nil
        } catch {
            return "\(error)"
        }
    }

    // ---------------- seed rotation ----------------
    // Pure passthrough (no repository state touched) - the view holds the
    // result locally while the user confirms they've saved the new phrase,
    // then hands the keystore bytes back via executeSeedRotation below.
    public func generateRotationCandidate() -> FfiKeystoreAndMnemonic {
        generateKeystoreWithMnemonic()
    }

    // There's no "account" to re-key here - owning a coin means knowing
    // its blinding factor, derived from the seed that sealed it.
    // "Replacing" a seed is therefore a real on-chain sweep: spend
    // everything the old seed owns into fresh outputs owned by a
    // brand-new seed, in one transaction.
    public func executeSeedRotation(newKeystoreBytes: [UInt8]) async -> String? {
        do {
            let fee = await currentFeeEstimate()
            let result = try rotateSeedTransaction(keystoreBytes: try requireKeystore(), storeBytes: storeBytes, newKeystoreBytes: newKeystoreBytes, fee: fee)
            try await api.submitTransaction(result.transactionJson)

            // Best-effort: hand the claimed name (if any) to the new
            // identity too - non-fatal if it fails, funds have already
            // moved by this point regardless.
            if let myName = state.claimedName {
                let newPubkeyHex = try? walletIdentityPubkeyHex(keystoreBytes: newKeystoreBytes)
                if let newPubkeyHex, let opJson = try? buildTransferNameRequest(keystoreBytes: try requireKeystore(), name: myName, newOwnerPubkeyHex: newPubkeyHex, resolveToPubkeyHex: newPubkeyHex) {
                    try? await api.transferName(opJson: opJson)
                }
            }

            let newStoreBytes = commitSend(storeBytes: walletStoreNew(), spentCommitmentsHex: [], dest: result.dest, change: nil)
            persistKeystore(newKeystoreBytes)
            persistStore(newStoreBytes)
            pushActivity("Rotated to a new seed phrase", "Moved balance to a new wallet")
            try await refreshBalance()
            return nil
        } catch {
            return "\(error)"
        }
    }
}

public enum HazeWalletError: Error, CustomStringConvertible {
    case noWallet
    public var description: String {
        switch self {
        case .noWallet: return "no wallet created yet"
        }
    }
}
