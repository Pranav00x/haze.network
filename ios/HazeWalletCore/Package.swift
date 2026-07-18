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
        .binaryTarget(name: "HazeCoreFFI", path: "../HazeCoreFFI.xcframework"),
        .target(
            name: "HazeWalletCore",
            dependencies: ["HazeCoreFFI"]
        ),
    ]
)
