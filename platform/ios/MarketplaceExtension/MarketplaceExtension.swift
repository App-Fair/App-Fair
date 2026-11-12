// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

// App Fair's marketplace extension (EXExtensionPointIdentifier `com.apple.marketplace.extension`).
// The system runs it while App Fair itself may not be running, to:
//
//   - decorate its requests to the marketplace server with the device's anonymous account;
//   - learn the newest version App Fair's catalogs offer for each installed app;
//   - fetch the install verification tokens its automatic updates need;
//   - report a failed request (App Fair has no sign-in, so there is nothing to repair).
//
// It reads what the app last shared through the App Group container (`marketplace.json`, written
// by `share_native` in marketplace/platform/ios/MarketplaceBridge.swift after every catalog
// sync), and asks the same token endpoint the app's install button does
// (docs/marketplace-server.md "Install tokens").

import ExtensionFoundation
import Foundation
import MarketplaceKit

/// What the app shares with the extension: `Shared` in marketplace/src/lib.rs.
struct AppFairShared: Codable {
    struct App: Codable {
        var appleItemId: UInt64
        var appleVersionId: UInt64?
        var bundleId: String
        var packageUrl: String
        var bundleVersion: String
        var shortVersion: String
        var shareUrl: String?
    }
    var server: String
    var account: String
    var apps: [App]

    static let appGroup = "group.org.appfair.AppFair"

    static func load() -> AppFairShared? {
        guard let dir = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier: appGroup),
            let data = try? Data(contentsOf: dir.appendingPathComponent("marketplace.json"))
        else { return nil }
        return try? JSONDecoder().decode(AppFairShared.self, from: data)
    }
}

/// The token exchange: the same request and response as the app's install button.
enum AppFairExtensionTokens {
    struct Request: Codable {
        struct App: Codable {
            var appleItemId: String
            var appleVersionId: String?
            var bundleId: String
            var alternativeDistributionPackage: String
            var update: Bool
        }
        var account: String
        var platform: String
        var osVersion: String
        var apps: [App]
    }

    struct Response: Codable {
        var tokens: [String: String]?
        var failures: [String: String]?
    }

    static func fetch(_ shared: AppFairShared, _ apps: [AppFairShared.App]) async throws
        -> [AppleItemID: String]
    {
        let base = shared.server.hasSuffix("/") ? shared.server : shared.server + "/"
        guard let url = URL(string: base + "v1/install-tokens") else { return [:] }
        let os = ProcessInfo.processInfo.operatingSystemVersion
        let body = Request(
            account: shared.account, platform: "iOS",
            osVersion: "\(os.majorVersion).\(os.minorVersion).\(os.patchVersion)",
            apps: apps.map {
                .init(appleItemId: String($0.appleItemId),
                      appleVersionId: $0.appleVersionId.map(String.init),
                      bundleId: $0.bundleId, alternativeDistributionPackage: $0.packageUrl,
                      update: true)
            })
        var request = URLRequest(url: url, timeoutInterval: 30)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(shared.account, forHTTPHeaderField: "X-AppFair-Account")
        request.httpBody = try JSONEncoder().encode(body)
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            return [:]
        }
        let decoded = try JSONDecoder().decode(Response.self, from: data)
        var out: [AppleItemID: String] = [:]
        for (id, token) in decoded.tokens ?? [:] {
            if let item = AppleItemID(id) { out[item] = token }
        }
        return out
    }
}

@main
struct AppFairMarketplaceExtension: MarketplaceAppExtension {
    init() {}

    func additionalHeaders(for request: URLRequest, account: String) async -> [String: String] {
        // The server authorizes downloads by the same anonymous account the app's installs used.
        ["X-AppFair-Account": account]
    }

    func availableAppVersions(forAppleItemIDs ids: [AppleItemID]) async -> [AppVersion] {
        guard let shared = AppFairShared.load() else { return [] }
        return ids.compactMap { id in
            guard let app = shared.apps.first(where: { $0.appleItemId == id }),
                  let version = app.appleVersionId
            else { return nil }
            return AppVersion(appleItemID: id, appleVersionID: version)
        }
    }

    func automaticUpdates(for installedAppVersions: [AppVersion]) async throws -> [AutomaticUpdate] {
        guard let shared = AppFairShared.load(), !shared.account.isEmpty else { return [] }
        // An app has an update when a catalog offers a different version than the one installed.
        // Catalogs that do not name the Apple version id (AltStore sources) leave it to the
        // server's `updates` endpoint, which the system polls on its own.
        let pending = installedAppVersions.compactMap { installed -> AppFairShared.App? in
            guard let app = shared.apps.first(where: { $0.appleItemId == installed.appleItemID }),
                  let offered = app.appleVersionId, offered != installed.appleVersionID
            else { return nil }
            return app
        }
        guard !pending.isEmpty else { return [] }
        let tokens = try await AppFairExtensionTokens.fetch(shared, pending)
        return pending.compactMap { app in
            guard let token = tokens[app.appleItemId], let url = URL(string: app.packageUrl)
            else { return nil }
            var update = AutomaticUpdate(appleItemID: app.appleItemId,
                                         alternativeDistributionPackage: url,
                                         account: shared.account,
                                         installVerificationToken: token)
            update.appShareURL = app.shareUrl.flatMap(URL.init(string:))
            return update
        }
    }

    func requestFailed(response: HTTPURLResponse) async -> Bool {
        // App Fair has no sign-in to renew, so there is nothing to retry with; the system reports
        // the failure as it would for any unreachable server.
        false
    }
}
