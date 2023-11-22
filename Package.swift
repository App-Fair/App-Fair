// swift-tools-version: 5.9
// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import PackageDescription

let package = Package(
    name: "appfair-app",
    defaultLocalization: "en",
    platforms: [.iOS(.v16), .macOS(.v13), .tvOS(.v16), .watchOS(.v9), .macCatalyst(.v16)],
    products: [
        .library(name: "AppFairApp", type: .dynamic, targets: ["AppFair"]),
        .library(name: "AppManagement", type: .dynamic, targets: ["AppManagement"]),
        .library(name: "AppLibrary", type: .dynamic, targets: ["AppLibrary"]),
        .library(name: "AppDistribution", type: .dynamic, targets: ["AppDistribution"]),
        .library(name: "AppSupport", type: .dynamic, targets: ["AppSupport"]),
    ],
    dependencies: [
        .package(url: "https://source.skip.tools/skip.git", from: "0.7.27"),
        .package(url: "https://source.skip.tools/skip-ui.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-motion.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-web.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-model.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-sql.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-xml.git", from: "0.0.0"),
        .package(url: "https://source.skip.tools/skip-foundation.git", from: "0.0.0")
    ],
    targets: [
        .target(name: "AppFair", dependencies: ["AppManagement", .product(name: "SkipUI", package: "skip-ui"), .product(name: "SkipMotion", package: "skip-motion"), .product(name: "SkipWeb", package: "skip-web")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppFairTests", dependencies: ["AppFair", .product(name: "SkipTest", package: "skip")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .target(name: "AppManagement", dependencies: ["AppLibrary", .product(name: "SkipModel", package: "skip-model")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppManagementTests", dependencies: ["AppManagement", .product(name: "SkipTest", package: "skip")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .target(name: "AppLibrary", dependencies: ["AppDistribution", .product(name: "SkipSQL", package: "skip-sql")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppLibraryTests", dependencies: ["AppLibrary", .product(name: "SkipTest", package: "skip")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .target(name: "AppDistribution", dependencies: ["AppSupport", .product(name: "SkipXML", package: "skip-xml")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppDistributionTests", dependencies: ["AppDistribution", .product(name: "SkipTest", package: "skip")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .target(name: "AppSupport", dependencies: [.product(name: "SkipFoundation", package: "skip-foundation")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
        .testTarget(name: "AppSupportTests", dependencies: ["AppSupport", .product(name: "SkipTest", package: "skip")], resources: [.process("Resources")], plugins: [.plugin(name: "skipstone", package: "skip")]),
    ]
)
