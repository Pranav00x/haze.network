import SwiftUI
import HazeWalletCore

struct MoreView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    @State private var nodeUrlField = ""
    @State private var explorerUrlField = ""
    @State private var stakeMinField = "1"
    @State private var stakeMessage: String?
    @State private var revealedKey: String?
    @State private var sweepKeyField = ""
    @State private var sweepMessage: String?

    @State private var showRotateConfirm = false
    @State private var rotateMnemonic: String?
    @State private var rotateNewKeystoreBytes: [UInt8]?
    @State private var rotateConfirmedSaved = false
    @State private var rotateBusy = false
    @State private var rotateMessage: String?

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("More").font(HazeFont.fraunces(24))

                    Text("Node").font(HazeFont.publicSans(17, weight: .semibold))
                    TextField("Node URL", text: $nodeUrlField).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task { await repo.setNodeUrl(nodeUrlField.trimmingCharacters(in: .whitespaces)) }
                    } label: { Text("Save node URL").frame(maxWidth: .infinity) }
                        .buttonStyle(HazeGhostButtonStyle())

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Block explorer").font(HazeFont.publicSans(17, weight: .semibold))
                    TextField("Explorer URL", text: $explorerUrlField).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        repo.setExplorerUrl(explorerUrlField.trimmingCharacters(in: .whitespaces))
                    } label: { Text("Save explorer URL").frame(maxWidth: .infinity) }
                        .buttonStyle(HazeGhostButtonStyle())

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Become a validator").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Stakes your single largest confirmed output. To actually propose blocks, run your own node with the revealed key.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    TextField("Minimum amount to stake", text: $stakeMinField).keyboardType(.numberPad).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task { stakeMessage = await repo.registerAsValidator(minValue: UInt64(stakeMinField) ?? 1) ?? "Registered as a validator." }
                    } label: { Text("Register as validator").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                    if let stakeMessage { Text(stakeMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }
                    Button {
                        revealedKey = try? repo.revealStakeKey(minValue: UInt64(stakeMinField) ?? 1)
                    } label: { Text("Reveal my validator key").font(HazeFont.publicSans(13)) }
                    if let revealedKey { Text(revealedKey).font(HazeFont.plexMono(11)).textSelection(.enabled) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Recover validator rewards").font(HazeFont.publicSans(17, weight: .semibold))
                    TextField("Validator stake key (hex)", text: $sweepKeyField).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task { sweepMessage = await repo.recoverValidatorRewards(stakeKeyHex: sweepKeyField.trimmingCharacters(in: .whitespaces)) ?? "Recovered rewards." }
                    } label: { Text("Recover rewards").frame(maxWidth: .infinity) }
                        .buttonStyle(HazeGhostButtonStyle())
                    if let sweepMessage { Text(sweepMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Rotate seed phrase").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Generates a brand new recovery phrase and moves your entire confirmed balance to it in one on-chain transaction. Your .haze name, if you have one, is transferred to the new phrase too.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)

                    if rotateMnemonic == nil {
                        Button {
                            rotateMessage = nil
                            if repo.state.confirmedBalance <= 0 {
                                rotateMessage = "No confirmed balance to move - nothing to rotate yet."
                            } else {
                                showRotateConfirm = true
                            }
                        } label: { Text("Start").frame(maxWidth: .infinity) }
                            .buttonStyle(HazeGhostButtonStyle())
                    } else {
                        HazeCard { Text(rotateMnemonic!).font(HazeFont.plexMono(14)) }
                        Toggle("I've written down the new recovery phrase", isOn: $rotateConfirmedSaved)
                            .font(HazeFont.publicSans(13))
                        Button {
                            guard let newBytes = rotateNewKeystoreBytes else { return }
                            rotateBusy = true
                            Task {
                                let result = await repo.executeSeedRotation(newKeystoreBytes: newBytes)
                                rotateBusy = false
                                if result == nil {
                                    rotateMessage = "Done - your funds are moving to the new phrase now (confirms shortly)."
                                    rotateMnemonic = nil
                                    rotateNewKeystoreBytes = nil
                                    rotateConfirmedSaved = false
                                } else {
                                    rotateMessage = result
                                }
                            }
                        } label: { Text(rotateBusy ? "Moving funds…" : "Move my funds to the new phrase").frame(maxWidth: .infinity) }
                            .buttonStyle(HazePrimaryButtonStyle())
                            .disabled(!rotateConfirmedSaved || rotateBusy)
                    }
                    if let rotateMessage { Text(rotateMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Button {
                        repo.lockWallet()
                    } label: { Text("Lock wallet").frame(maxWidth: .infinity) }
                        .buttonStyle(HazeDangerButtonStyle())
                }
                .padding(24)
            }
        }
        .onAppear {
            nodeUrlField = repo.state.nodeUrl
            explorerUrlField = repo.state.explorerUrl
        }
        .alert("Rotate seed phrase?", isPresented: $showRotateConfirm) {
            Button("Cancel", role: .cancel) {}
            Button("Continue") {
                let generated = repo.generateRotationCandidate()
                rotateMnemonic = generated.mnemonic
                rotateNewKeystoreBytes = generated.keystoreBytes
            }
        } message: {
            Text("This creates a new recovery phrase and moves your entire balance to it in one transaction. Your current phrase will no longer control these funds afterward.")
        }
    }
}

struct HazeDangerButtonStyle: ButtonStyle {
    @Environment(\.hazePalette) private var palette
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HazeFont.publicSans(15, weight: .semibold))
            .padding(.vertical, 14)
            .background(palette.danger.opacity(configuration.isPressed ? 0.8 : 1))
            .foregroundStyle(Color.white)
            .clipShape(RoundedRectangle(cornerRadius: 14))
    }
}
