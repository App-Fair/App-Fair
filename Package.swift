// swift-tools-version: 5.9
// This is free software: you can redistribute and/or modify it
// under the terms of the GNU General Public License 3.0
// as published by the Free Software Foundation https://fsf.org
import PackageDescription

let package = Package(
    name: "app-fair-app",
    defaultLocalization: "en",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "AppFairApp", type: .dynamic, targets: ["AppFair"]),
        .library(name: "AppFairModel", type: .dynamic, targets: ["AppFairModel"]),
    ],
    dependencies: [
        .package(url: "https://source.skip.tools/skip.git", from: "1.0.0"),
        .package(url: "https://source.skip.tools/skip-foundation.git", from: "1.0.0"),
        .package(url: "https://source.skip.tools/skip-model.git", from: "1.0.0"),
        .package(url: "https://source.skip.tools/skip-ui.git", from: "1.0.0"),
        .package(url: "https://source.skip.tools/skip-web.git", "0.0.0"..<"2.0.0"),
        .package(url: "https://source.skip.tools/skip-sql.git", "0.0.0"..<"2.0.0"),
        //.package(url: "https://source.skip.tools/skip-motion.git", "0.0.0"..<"2.0.0"),
        //.package(url: "https://source.skip.tools/skip-xml.git", "0.0.0"..<"2.0.0"),
        //.package(url: "https://source.skip.tools/skip-zip.git", "0.0.0"..<"2.0.0"),
    ],
    targets: [
        .target(name: "AppFair", dependencies: [
            "AppFairModel",
            .product(name: "SkipUI", package: "skip-ui"),
            //.product(name: "SkipMotion", package: "skip-motion"),
            .product(name: "SkipWeb", package: "skip-web"),
        ], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppFairTests", dependencies: [
            "AppFair",
            .product(name: "SkipTest", package: "skip")
        ], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),

        .target(name: "AppFairModel", dependencies: [
            .product(name: "SkipFoundation", package: "skip-foundation"),
            .product(name: "SkipModel", package: "skip-model"),
            .product(name: "SkipSQL", package: "skip-sql"),
            //.product(name: "SkipXML", package: "skip-xml"),
            //.product(name: "SkipZip", package: "skip-zip"),
        ], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppFairModelTests", dependencies: [
            "AppFairModel",
            .product(name: "SkipTest", package: "skip")
        ], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
    ]
)
