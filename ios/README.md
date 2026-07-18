# Haze Wallet - iOS

Same core as the Android/web wallets (Rust, via UniFFI), same fog/amber
visual language as the Android app - just no compiled/signed app yet,
since that needs a real Mac with Xcode, which this dev environment
doesn't have. Everything here compiles and is verified in CI on a
GitHub-hosted macOS runner (`.github/workflows/ios.yml`); the remaining
step is wiring it into an actual Xcode app target, which is a one-time,
mostly-GUI setup.

## What's here

- `HazeWalletCore/` - a Swift Package with all the wallet logic
  (`WalletRepository`, `NodeApi`, `SecureStorage`) plus the Rust FFI
  bindings, generated fresh by CI. `swift build` in this directory is
  what CI runs to prove it actually compiles.
- `HazeWalletApp/` - SwiftUI view source (`HazeWalletApp.swift`,
  `ContentView.swift`, and one file per screen) plus the four font files
  the Android app already uses (`Fonts/`). Not yet part of a buildable
  Xcode target - see below.

## One-time setup on a Mac

1. Run the `iOS` GitHub Actions workflow (or push a change under
   `src/ffi.rs`, `core/`, `crypto/`, `wallet/`, or `ios/` to trigger it),
   download the `HazeCoreFFI-xcframework` artifact, and unzip
   `HazeCoreFFI.xcframework` into `ios/` (next to `HazeWalletCore/`) -
   `HazeWalletCore/Package.swift` expects it at `../HazeCoreFFI.xcframework`
   relative to the package root.
2. In Xcode: File > New > Project > iOS > App. Name it "Haze Wallet",
   interface SwiftUI, minimum deployment target iOS 16.
3. Delete the generated `ContentView.swift` and app entry point Xcode
   creates; drag every file from `HazeWalletApp/` (including `Fonts/`)
   into the new project, "Copy items if needed" checked.
4. For each `.ttf` in `Fonts/`: add it to the target's
   "Fonts provided by application" (`UIAppFonts`) list in Info.plist,
   and register the family names Xcode reports for them
   (`Fraunces`, `PublicSans`, `IBMPlexMono`) - `HazeTheme.swift`
   references those exact family names.
5. File > Add Package Dependencies > Add Local... > select
   `ios/HazeWalletCore`.
6. Set a bundle identifier and signing team (Signing & Capabilities tab)
   - this is the one step that's genuinely only doable interactively in
   Xcode, there's no headless equivalent.
7. Build and run.

## Scope of this first pass

Onboarding, wallet home, send (self-pay + pay-to-name), receive,
names (claim/lookup/transfer), activity history, and settings
(node/explorer URL, validator staking, seed rotation, lock) - the same
set Android had before its marketplace screens were added. Marketplace,
minting, and collection drops aren't ported to iOS yet; same shape as
Android's, straightforward to add once this base is verified running on
a device.
