import SwiftUI
import HazeWalletCore

struct SendView: View {
    @EnvironmentObject var repo: WalletRepository
    @Environment(\.hazePalette) private var palette

    @State private var selfAmount = ""
    @State private var selfMessage: String?
    @State private var toName = ""
    @State private var nameAmount = ""
    @State private var nameMessage: String?
    @State private var sending = false

    var body: some View {
        ZStack {
            HazeBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Send").font(HazeFont.fraunces(24))

                    Text("Self-pay").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Splits/consolidates your own confirmed UTXOs.").font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    TextField("Amount", text: $selfAmount).keyboardType(.numberPad).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        Task {
                            do {
                                try await repo.selfPay(amount: UInt64(selfAmount) ?? 0)
                                selfMessage = "Broadcast successfully. Balance will update once mined."
                            } catch {
                                selfMessage = "\(error)"
                            }
                        }
                    } label: { Text("Send").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                    if let selfMessage { Text(selfMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }

                    Divider().background(palette.hairline).padding(.vertical, 8)

                    Text("Send to a name").font(HazeFont.publicSans(17, weight: .semibold))
                    Text("Sends directly to someone's registered .haze name.").font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint)
                    TextField("Recipient's name", text: $toName).textFieldStyle(HazeTextFieldStyle())
                    TextField("Amount", text: $nameAmount).keyboardType(.numberPad).textFieldStyle(HazeTextFieldStyle())
                    Button {
                        sending = true
                        nameMessage = "Sending…"
                        Task {
                            let err = await repo.sendToName(name: toName.trimmingCharacters(in: .whitespaces), amount: UInt64(nameAmount) ?? 0)
                            nameMessage = err ?? "Sent."
                            sending = false
                        }
                    } label: { Text(sending ? "Sending…" : "Send").frame(maxWidth: .infinity) }
                        .buttonStyle(HazePrimaryButtonStyle())
                        .disabled(sending)
                    if let nameMessage { Text(nameMessage).font(HazeFont.publicSans(13)).foregroundStyle(palette.inkFaint) }
                }
                .padding(24)
            }
        }
    }
}
