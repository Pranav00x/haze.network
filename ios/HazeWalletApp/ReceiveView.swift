import SwiftUI
import HazeWalletCore

struct ReceiveView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    @State private var incomingSlate = ""
    @State private var responseOut: String?
    @State private var error: String?

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Receive").font(HazeFont.fraunces(24))

                    Text("Your address").font(HazeFont.publicSans(17, weight: .semibold))
                    Text(repo.state.claimedName.map { "\($0).haze" } ?? "Claim a name in the Names tab to receive payments by name.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Receive a payment").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Paste a slate someone sent you directly. This builds your response - send it back to them.")
                        .font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    TextField("Incoming slate JSON", text: $incomingSlate, axis: .vertical)
                        .lineLimit(6...10)
                        .textFieldStyle(HazeTextFieldStyle())
                    Button {
                        do {
                            responseOut = try repo.respondToPastedSlate(incomingSlate)
                            error = nil
                        } catch {
                            self.error = "\(error)"
                        }
                    } label: { Text("Respond").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                    if let error { Text(error).foregroundStyle(palette.danger).font(HazeFont.publicSans(13)) }
                    if let responseOut {
                        Text("Send this back to the sender:").font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                        Text(responseOut).font(HazeFont.plexMono(12)).textSelection(.enabled)
                    }
                }
                .padding(24)
            }
        }
    }
}
