// swift-tools-version: 6.0
// Vendored facet/vox Swift runtime — see VENDOR.md for provenance.
//
// The phon codec (schema/IR/interpreter) + the vox connection runtime
// (handshake, lanes, channels, typed calls) so Apple-platform clients speak
// the same wire as `#[architect::rpc]` services. The upstream JIT targets
// (PhonJIT / VoxRuntimeJIT) are deliberately dropped: the interpreter is
// plenty for control-surface traffic and the JIT stencils are macOS-only.
//
// Platforms: macOS 15 / iOS 18 / watchOS 11 (needs native UInt128 /
// String(validating:)). NIO is only exercised by the TCP/Unix transports;
// watchOS clients use a URLSession link instead (Apple TN3135 forbids raw
// sockets there) — the modules still compile everywhere.
import PackageDescription

let package = Package(
    name: "facet-swift",
    platforms: [
        .macOS(.v15),
        .iOS(.v18),
        .watchOS(.v11),
    ],
    products: [
        .library(name: "Phon", targets: ["Phon"]),
        .library(name: "PhonSchema", targets: ["PhonSchema"]),
        .library(name: "PhonIR", targets: ["PhonIR"]),
        .library(name: "PhonEngine", targets: ["PhonEngine"]),
        .library(name: "VoxRuntime", targets: ["VoxRuntime"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-nio.git", from: "2.99.0")
    ],
    targets: [
        .target(
            name: "CBlake3",
            path: "phon/swift/cblake3/Sources/CBlake3",
            cSettings: [
                .define("BLAKE3_USE_NEON", to: "0"),
                .define("BLAKE3_NO_SSE2"),
                .define("BLAKE3_NO_SSE41"),
                .define("BLAKE3_NO_AVX2"),
                .define("BLAKE3_NO_AVX512"),
            ]
        ),
        .target(
            name: "PhonSchema",
            dependencies: ["CBlake3"],
            path: "phon/swift/phon-schema/Sources/PhonSchema"
        ),
        .target(
            name: "PhonIR",
            dependencies: ["PhonSchema"],
            path: "phon/swift/phon-ir/Sources/PhonIR"
        ),
        .target(
            name: "PhonEngine",
            dependencies: ["PhonSchema", "PhonIR"],
            path: "phon/swift/phon-engine/Sources/PhonEngine"
        ),
        .target(
            name: "Phon",
            dependencies: ["PhonSchema", "PhonEngine"],
            path: "phon/swift/phon/Sources/Phon"
        ),
        .target(
            name: "VoxRuntime",
            dependencies: [
                .product(name: "NIO", package: "swift-nio"),
                .product(name: "NIOCore", package: "swift-nio"),
                .product(name: "NIOPosix", package: "swift-nio"),
                "PhonSchema",
                "PhonIR",
                "PhonEngine",
            ],
            path: "vox/swift/vox-runtime/Sources/VoxRuntime",
            resources: [
                .copy("wireMessageSchemas.bin")
            ]
        ),
        .testTarget(
            name: "VoxRuntimeTests",
            dependencies: ["VoxRuntime", "PhonSchema"],
            path: "vox/swift/vox-runtime/Tests/VoxRuntimeTests"
        ),
    ]
)
