import SwiftUI
import HazeWalletCore

struct WalletHomeView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette
    @State private var faucetMessage: String?

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(repo.state.claimedName.map { "\($0).haze" } ?? "Haze Wallet")
                        .font(HazeFont.publicSans(16, weight: .semibold))
                        .foregroundStyle(repo.state.claimedName != nil ? palette.amber : palette.ink)

                    HazeCard {
                        HStack {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("CONFIRMED").font(HazeFont.publicSans(10, weight: .semibold)).foregroundStyle(palette.inkFaint)
                                Text("\(repo.state.confirmedBalance)").font(HazeFont.fraunces(30))
                            }
                            Spacer()
                            VStack(alignment: .leading, spacing: 4) {
                                Text("PENDING").font(HazeFont.publicSans(10, weight: .semibold)).foregroundStyle(palette.inkFaint)
                                Text("\(repo.state.pendingBalance)").font(HazeFont.fraunces(30)).foregroundStyle(palette.inkFaint)
                            }
                        }
                    }

                    Button {
                        Task {
                            do {
                                try await repo.claimFaucet(amount: 500)
                                faucetMessage = "Received 500. Refreshing balance…"
                            } catch {
                                faucetMessage = "Faucet unavailable: \(error)"
                            }
                        }
                    } label: {
                        Text("Get devnet funds").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazePrimaryButtonStyle())

                    if let faucetMessage { Text(faucetMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }

                    Button {
                        Task { try? await repo.refreshBalance() }
                    } label: {
                        Text("Refresh balance").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(HazeGhostButtonStyle())
                }
                .padding(24)
            }
        }
    }
}
