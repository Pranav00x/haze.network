import SwiftUI
import HazeWalletCore

struct NamesView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    @State private var claimField = ""
    @State private var claimMessage: String?
    @State private var lookupField = ""
    @State private var lookupResult: String?
    @State private var transferName = ""
    @State private var transferTo = ""
    @State private var transferMessage: String?

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Names").font(HazeFont.fraunces(24))

                    Text("Claim a .haze name").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Permanent, first-come-first-served. Free to claim - sponsored by the network.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    TextField("Name", text: $claimField).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task { claimMessage = await repo.claimName(claimField.trimmingCharacters(in: .whitespaces)) ?? "Claiming - waiting for it to be mined…" }
                    } label: { Text("Claim").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                    if let claimMessage { Text(claimMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Look up a name").font(HazeFont.publicSans(17, weight: .semibold))
                    TextField("Name", text: $lookupField).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task {
                            let result = await repo.lookupName(lookupField.trimmingCharacters(in: .whitespaces))
                            lookupResult = result.map { "\($0)" } ?? "not registered"
                        }
                    } label: { Text("Look up").frame(maxWidth: .infinity) }
                        .buttonStyle(HazeGhostButtonStyle())
                    if let lookupResult { Text(lookupResult).font(HazeFont.plexMono(12)).foregroundStyle(palette.inkFaint) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Transfer a name you own").font(HazeFont.publicSans(17, weight: .semibold))
                    TextField("Name", text: $transferName).textFieldStyle(HazeTextFieldStyle())
                    TextField("New owner pubkey (hex)", text: $transferTo).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task { transferMessage = await repo.transferName(transferName.trimmingCharacters(in: .whitespaces), newOwnerPubkeyHex: transferTo.trimmingCharacters(in: .whitespaces)) ?? "Transferred." }
                    } label: { Text("Transfer").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                    if let transferMessage { Text(transferMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }
                }
                .padding(24)
            }
        }
    }
}
