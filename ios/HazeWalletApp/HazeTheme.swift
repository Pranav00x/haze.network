import SwiftUI

// The same fog/mist palette as the web wallet and Android app (oklch
// converted to sRGB) - see HazeTheme.kt's HazeDark/HazeLight for the
// source of truth this mirrors 1:1.
struct HazePalette {
    let fog0, fog1, fog2, fog3: Color
    let mist, ink, amber, ok, danger: Color
    let cardVeil, hairline: Color
    let isDark: Bool

    static let dark = HazePalette(
        fog0: Color(hex: 0x17161D), fog1: Color(hex: 0x1E1D25), fog2: Color(hex: 0x28262F), fog3: Color(hex: 0x37343F),
        mist: Color(hex: 0x9C9AC7), ink: Color(hex: 0xEEEAE1), amber: Color(hex: 0xE2A75E),
        ok: Color(hex: 0x7ED9A3), danger: Color(hex: 0xEA7561),
        cardVeil: Color(hex: 0x302E3A), hairline: Color.white.opacity(0.09),
        isDark: true
    )

    static let light = HazePalette(
        fog0: Color(hex: 0xF9F9FA), fog1: Color(hex: 0xF2F1F4), fog2: Color(hex: 0xE6E4E9), fog3: Color(hex: 0xD1CFD8),
        mist: Color(hex: 0x6D64A8), ink: Color(hex: 0x302D38), amber: Color(hex: 0xB97A2E),
        ok: Color(hex: 0x2F8F5D), danger: Color(hex: 0xC2452C),
        cardVeil: Color(hex: 0xEDEBF0), hairline: Color.black.opacity(0.08),
        isDark: false
    )

    var inkFaint: Color { ink.opacity(isDark ? 0.34 : 0.42) }
}

extension Color {
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255
        )
    }
}

enum HazeFont {
    static func fraunces(_ size: CGFloat, weight: Font.Weight = .semibold) -> Font {
        .custom("Fraunces", size: size).weight(weight)
    }
    static func publicSans(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .custom("PublicSans", size: size).weight(weight)
    }
    static func plexMono(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .custom("IBMPlexMono", size: size).weight(weight)
    }
}

private struct HazePaletteKey: EnvironmentKey {
    static let defaultValue = HazePalette.dark
}

extension EnvironmentValues {
    var hazePalette: HazePalette {
        get { self[HazePaletteKey.self] }
        set { self[HazePaletteKey.self] = newValue }
    }
}

/// The glass card treatment used everywhere - veil fill + hairline border,
/// mirrors HazeCard from the Android app's HazeComponents.kt.
struct HazeCard<Content: View>: View {
    @Environment(\.hazePalette) private var palette
    let content: () -> Content
    init(@ViewBuilder content: @escaping () -> Content) { self.content = content }

    var body: some View {
        content()
            .padding(16)
            .background(palette.cardVeil.opacity(palette.isDark ? 0.5 : 0.7))
            .overlay(RoundedRectangle(cornerRadius: 18).stroke(palette.hairline, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: 18))
    }
}

struct HazeBackground: View {
    @Environment(\.hazePalette) private var palette
    var body: some View {
        palette.fog0
            .overlay(
                RadialGradient(
                    colors: [palette.mist.opacity(palette.isDark ? 0.22 : 0.14), .clear],
                    center: UnitPoint(x: 0.1, y: 0),
                    startRadius: 0, endRadius: 500
                )
            )
            .ignoresSafeArea()
    }
}
