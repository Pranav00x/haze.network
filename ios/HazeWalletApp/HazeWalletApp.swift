import SwiftUI
import HazeWalletCore

@main
struct HazeWalletApp: App {
    @StateObject private var repo = WalletRepository()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(repo)
                .environment(\.hazePalette, .dark) // TODO: derive from ColorScheme once light palette is verified in Xcode
                .preferredColorScheme(.dark)
        }
    }
}
