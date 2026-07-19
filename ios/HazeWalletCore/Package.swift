// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "HazeWalletCore",
    platforms: [.iOS(.v16)],
    products: [
        .library(name: "HazeWalletCore", targets: ["HazeWalletCore"]),
    ],
    targets: [
        // Built by .github/workflows/ios.yml (cargo build --target
        // aarch64-apple-ios[-sim] + xcodebuild -create-xcframework) -
        // not checked into git, since it's a compiled artifact that
        // needs a Mac toolchain to produce. Run that workflow (or its
        // steps locally on a Mac) before opening this package in Xcode.
        //
        // Named haze_coreFFI (not e.g. HazeCoreFFI) deliberately - for a
        // plain library+headers XCFramework (no compiled .framework of
        // its own), SPM exposes the binary target's module under exactly
        // the name given to `.binaryTarget(name:)`, regardless of what
        // the modulemap inside declares. uniffi-bindgen's generated
        // Swift file hardcodes `import haze_coreFFI` (crate name +
        // "FFI") - this name has to match that exactly or the import
        // silently fails to resolve (seen as "cannot find type
        // 'RustBuffer'" etc, not an import error, which is what makes it
        // confusing).
        .binaryTarget(name: "haze_coreFFI", path: "../haze_coreFFI.xcframework"),
        .target(
            name: "HazeWalletCore",
            dependencies: ["haze_coreFFI"]
        ),
    ]
)
