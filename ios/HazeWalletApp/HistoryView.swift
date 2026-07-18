import SwiftUI
import HazeWalletCore

struct HistoryView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Activity").font(HazeFont.fraunces(24))
                    Text("Everything this wallet has sent, received, or registered - newest first.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)

                    if repo.state.activity.isEmpty {
                        Text("No activity yet.").font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    } else {
                        ForEach(repo.state.activity) { entry in
                            HazeCard {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(entry.title).font(HazeFont.publicSans(14, weight: .medium))
                                    if !entry.detail.isEmpty {
                                        Text(entry.detail).font(HazeFont.publicSans(12)).foregroundStyle(palette.inkFaint)
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(24)
            }
        }
    }
}
