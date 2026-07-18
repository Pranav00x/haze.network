import SwiftUI
import HazeWalletCore

struct OnboardingView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    private enum Mode { case choose, mnemonic, restore }
    @State private var mode: Mode = .choose
    @State private var generatedMnemonic = ""
    @State private var confirmed = false
    @State private var restorePhrase = ""
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Haze Wallet").font(HazeFont.fraunces(28))

                switch mode {
                case .choose:
                    Text("Generates a real keystore (via iOS's secure RNG), stored in the Keychain.")
                        .font(HazeFont.publicSans(14)).foregroundStyle(palette.inkFaint)
                    Button {
                        generatedMnemonic = repo.createWallet()
                        mode = .mnemonic
                    } label: {
                        Text("Create Wallet").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazePrimaryButtonStyle())

                    Button { mode = .restore } label: {
                        Text("Restore from recovery phrase").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazeGhostButtonStyle())

                case .mnemonic:
                    Text("Save your recovery phrase").font(HazeFont.publicSans(18, weight: .semibold))
                    Text("These 12 words are the ONLY way to recover this wallet. Anyone with this phrase can spend your funds - write it down and keep it private. Haze cannot recover it for you.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    HazeCard { Text(generatedMnemonic).font(HazeFont.plexMono(14)) }
                    Toggle("I've written down my recovery phrase", isOn: $confirmed)
                        .font(HazeFont.publicSans(14))
                    Button {
                        mode = .choose // wallet already created; ContentView will pick up hasWallet
                    } label: {
                        Text("Continue").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazePrimaryButtonStyle())
                    .disabled(!confirmed)

                case .restore:
                    Text("Restore from recovery phrase").font(HazeFont.publicSans(18, weight: .semibold))
                    TextField("12-word recovery phrase", text: $restorePhrase, axis: .vertical)
                        .textFieldStyle(HazeTextFieldStyle())
                    if let error { Text(error).foregroundStyle(palette.danger).font(HazeFont.publicSans(12)) }
                    Button {
                        busy = true
                        Task {
                            do {
                                try await repo.restoreWallet(mnemonic: restorePhrase)
                            } catch {
                                self.error = "\(error)"
                            }
                            busy = false
                        }
                    } label: {
                        Text(busy ? "Restoring…" : "Restore").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazePrimaryButtonStyle())
                    .disabled(busy)
                }
            }
            .padding(24)
        }
    }
}

// ---------------- shared button/field styles ----------------

struct HazePrimaryButtonStyle: ButtonStyle {
    @Environment(\.hazePalette) private var palette
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HazeFont.publicSans(15, weight: .semibold))
            .padding(.vertical, 14)
            .background(palette.amber.opacity(configuration.isPressed ? 0.8 : 1))
            .foregroundStyle(palette.fog0)
            .clipShape(RoundedRectangle(cornerRadius: 14))
    }
}

struct HazeGhostButtonStyle: ButtonStyle {
    @Environment(\.hazePalette) private var palette
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(HazeFont.publicSans(15, weight: .medium))
            .padding(.vertical, 14)
            .foregroundStyle(palette.ink)
            .overlay(RoundedRectangle(cornerRadius: 14).stroke(palette.fog3, lineWidth: 1))
    }
}

struct HazeTextFieldStyle: TextFieldStyle {
    @Environment(\.hazePalette) private var palette
    func _body(configuration: TextField<Self._Label>) -> some View {
        configuration
            .font(HazeFont.plexMono(14))
            .padding(12)
            .background(palette.fog1)
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.fog3, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}
