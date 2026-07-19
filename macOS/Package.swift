// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "QuickShare",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(name: "quickshare-daemon", targets: ["QuickShareDaemon"]),
    ],
    dependencies: [],
    targets: [
        .target(
            name: "CQuickShare",
            dependencies: [],
            linkerSettings: [
                .unsafeFlags(["-L", "Bridge"]),
                .linkedLibrary("quickshare_core"),
                .linkedFramework("CoreBluetooth"),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("IOKit"),
            ]
        ),
        .target(
            name: "QuickShareBridge",
            dependencies: ["CQuickShare"]
        ),
        .executableTarget(
            name: "QuickShareDaemon",
            dependencies: ["QuickShareBridge"]
        ),
    ]
)
