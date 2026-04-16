// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "XIFtySwift",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .library(name: "XIFtySwift", targets: ["XIFtySwift"]),
    ],
    targets: [
        .target(
            name: "CXIFty",
            path: "Sources/CXIFty",
            publicHeadersPath: "include"
        ),
        .target(
            name: "XIFtySwift",
            dependencies: ["CXIFty"],
            path: "Sources/XIFtySwift",
            linkerSettings: [
                .unsafeFlags([
                    "-L", "../../target/debug",
                    "-lxifty_ffi",
                ])
            ]
        ),
        .testTarget(
            name: "XIFtySwiftTests",
            dependencies: ["XIFtySwift"],
            path: "Tests/XIFtySwiftTests",
            linkerSettings: [
                .unsafeFlags([
                    "-L", "../../target/debug",
                    "-lxifty_ffi",
                ])
            ]
        ),
    ]
)
