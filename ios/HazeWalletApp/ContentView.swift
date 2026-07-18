import SwiftUI
import HazeWalletCore

struct ContentView: View {
    @EnvironmentObject var repo: WalletRepository

    var body: some View {
        ZStack {
            HazeBackground()
            if !repo.state.hasWallet {
                OnboardingView()
            } else {
                MainTabView()
            }
        }
        .task {
            try? await repo.refreshBalance()
        }
    }
}

struct MainTabView: View {
    var body: some View {
        TabView {
            WalletHomeView()
                .tabItem { Label("Wallet", systemImage: "house.fill") }
            SendView()
                .tabItem { Label("Send", systemImage: "paperplane.fill") }
            ReceiveView()
                .tabItem { Label("Receive", systemImage: "tray.and.arrow.down.fill") }
            NamesView()
                .tabItem { Label("Names", systemImage: "at") }
            HistoryView()
                .tabItem { Label("History", systemImage: "clock.fill") }
            MoreView()
                .tabItem { Label("More", systemImage: "ellipsis.circle.fill") }
        }
        .tint(HazePalette.dark.amber)
    }
}
