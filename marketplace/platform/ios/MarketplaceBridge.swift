// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

// The iOS arm of app-fair-marketplace (src/lib.rs): App Fair's installs, updates, launches and
// deletions through Apple's MarketplaceKit, which is Swift-only and so unreachable from objc2.
//
// Three parts share this file because they share state:
//   - the declared bridge functions (`configure_native` … `share_native`), called from Rust;
//   - the library observer, which streams `AppLibrary` snapshots and button events back to Rust
//     through `observe_native`'s emit token;
//   - the `DayView_appfair_marketplace_action` SwiftUI provider: MarketplaceKit's `ActionButton`,
//     the only control from which the system accepts an install, embedded in a Day page through
//     day-piece-swiftui.
//
// An install is confirmed inside the button's `confirmInstall` closure, which asks App Fair's
// marketplace server for an install verification token (docs/marketplace-server.md). The private
// key that signs it never reaches the device.

import Foundation
import MarketplaceKit
import Observation
import Security
import SwiftUI
import UIKit

// MARK: - Errors and configuration

struct AppFairMarketError: Error, CustomStringConvertible, LocalizedError {
    let description: String
    init(_ description: String) { self.description = description }
    var errorDescription: String? { description }
}

/// Where the marketplace server lives and the App Group the app shares with its marketplace
/// extension. Set once at launch by `configure_native`.
struct AppFairMarketConfig: Codable {
    var server: String
    var appGroup: String
}

private let appFairMarketLock = NSLock()
nonisolated(unsafe) private var appFairMarketConfigValue = AppFairMarketConfig(server: "", appGroup: "")

private var appFairMarketConfig: AppFairMarketConfig {
    appFairMarketLock.lock()
    defer { appFairMarketLock.unlock() }
    return appFairMarketConfigValue
}

private func appFairDecode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
    do {
        return try JSONDecoder().decode(type, from: Data(json.utf8))
    } catch {
        throw AppFairMarketError("\(T.self): \(error)")
    }
}

private func appFairEncode<T: Encodable>(_ value: T) -> String {
    guard let data = try? JSONEncoder().encode(value) else { return "{}" }
    return String(decoding: data, as: UTF8.self)
}

// MARK: - Bridge functions

func configure_native(config: String) throws {
    let decoded = try appFairDecode(AppFairMarketConfig.self, config)
    appFairMarketLock.lock()
    appFairMarketConfigValue = decoded
    appFairMarketLock.unlock()
}

/// What the marketplace can do on this device: who distributed App Fair itself, the device's
/// catalog region, and the highest age rating it may install.
struct AppFairMarketStatus: Codable {
    var distributor: String?
    var catalogRegion: String?
    var maximumAgeRating: Int?
    var error: String?
}

func status_native(done: UInt64) throws {
    Task {
        var status = AppFairMarketStatus()
        do {
            switch try await AppDistributor.current {
            case .appStore: status.distributor = "appStore"
            case .testFlight: status.distributor = "testFlight"
            case .marketplace(let id): status.distributor = "marketplace:\(id)"
            case .web: status.distributor = "web"
            case .other: status.distributor = "other"
            @unknown default: status.distributor = "unknown"
            }
        } catch {
            status.error = "\(error)"
        }
        if #available(iOS 26.2, *) {
            status.catalogRegion = await AppLibrary.current.catalogRegion
        }
        if #available(iOS 26.0, *) {
            status.maximumAgeRating = await MainActor.run { AppLibrary.current.maximumAllowedAgeRating }
        }
        status_native_complete(done, appFairEncode(status))
    }
}

func account_native(done: UInt64) throws {
    let id = try AppFairAccount.current()
    account_native_complete(done, id)
}

func observe_native(emit: UInt64) throws {
    DispatchQueue.main.async {
        AppFairLibraryObserver.shared.add(emit)
    }
}

/// Apple's own `InstallRequirements`, decoded from the catalog's fields so the device answers
/// exactly as the installer will.
struct AppFairRequirements: Codable {
    var minimumSystemVersion: String?
    var requiredDeviceCapabilities: [String]?
    var ageRatingRank: Int?
    var expectedInstallSize: UInt64?
}

func satisfies_native(requirements: String, done: UInt64) throws {
    let decoded = try appFairDecode(AppFairRequirements.self, requirements)
    // `satisfiedByDevice()` asks a system daemon over synchronous XPC, so it never runs on the
    // caller's thread (Day's UI thread): a slow or absent reply would freeze the app. Where no
    // reply ever comes (the simulator), the token simply stays unanswered and the caller keeps
    // its default.
    DispatchQueue.global(qos: .userInitiated).async {
        var r = InstallRequirements()
        r.minimumSystemVersion = decoded.minimumSystemVersion
        r.requiredDeviceCapabilities = decoded.requiredDeviceCapabilities.map(Set.init)
        r.ageRatingRank = decoded.ageRatingRank
        r.expectedInstallSize = decoded.expectedInstallSize
        satisfies_native_complete(done, r.satisfiedByDevice() ? 1 : 0)
    }
}

func cancel_native(apple_item_id: Int64) throws {
    DispatchQueue.main.async {
        let app = AppLibrary.current.app(forAppleItemID: AppleItemID(apple_item_id))
        if let progress = app.installation?.progress, progress.isCancellable {
            progress.cancel()
        }
    }
}

/// Write what the marketplace extension needs (the account, the server, and the newest version
/// the catalogs offer for each app) into the App Group container both targets read.
func share_native(shared: String) throws {
    let group = appFairMarketConfig.appGroup
    guard !group.isEmpty,
          let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
    else {
        throw AppFairMarketError("the App Group \(group) is not available to this build")
    }
    try Data(shared.utf8).write(to: dir.appendingPathComponent("marketplace.json"), options: .atomic)
}

// MARK: - The anonymous account

/// App Fair has no sign-in. MarketplaceKit still needs an `account` on every install, so each
/// device gets a random identifier, kept in the keychain (so it survives deleting and reinstalling
/// App Fair) and mirrored into the App Group defaults for the extension, which has no access to
/// the app's keychain items.
enum AppFairAccount {
    private static let service = "org.appfair.marketplace"
    private static let key = "account"
    nonisolated(unsafe) private static var cached: String?

    static func current() throws -> String {
        appFairMarketLock.lock()
        defer { appFairMarketLock.unlock() }
        if let cached { return cached }
        let id: String
        if let stored = read() {
            id = stored
        } else {
            id = UUID().uuidString.lowercased()
            try write(id)
        }
        cached = id
        let group = appFairMarketConfigValue.appGroup
        if !group.isEmpty {
            UserDefaults(suiteName: group)?.set(id, forKey: key)
        }
        return id
    }

    private static func query() -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: key]
    }

    private static func read() -> String? {
        var q = query()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
              let data = out as? Data,
              let value = String(data: data, encoding: .utf8), !value.isEmpty
        else { return nil }
        return value
    }

    private static func write(_ id: String) throws {
        var q = query()
        q[kSecValueData as String] = Data(id.utf8)
        // Readable after the first unlock, so a background update check can still identify the
        // device; never synced to other devices, which have accounts of their own.
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(q as CFDictionary, nil)
        guard status == errSecSuccess || status == errSecDuplicateItem else {
            throw AppFairMarketError("the keychain refused the account id (\(status))")
        }
    }
}

// MARK: - The library observer

/// One installed or installing app, as Rust reads it.
struct AppFairLibraryApp: Codable {
    struct Installed: Codable {
        var version: String
        var shortVersion: String
        var appleVersionId: UInt64
        var account: String?
    }
    struct Progress: Codable {
        var fraction: Double
        var completed: Int64
        var total: Int64
        var cancellable: Bool
    }
    var id: UInt64
    var installed: Installed?
    var installing: Bool
    var updating: Bool
    var progress: Progress?
    var error: String?
}

/// Everything `observe_native` delivers: a library snapshot, or what a button's confirmation did.
struct AppFairEvent: Codable {
    var type: String
    var loading: Bool?
    var apps: [AppFairLibraryApp]?
    var ids: [UInt64]?
    var message: String?
}

/// Streams `AppLibrary` to every Rust listener: a snapshot whenever the library's observable state
/// changes, and four a second while something installs, because an installation's `Progress` is
/// key-value observed rather than `@Observable`.
@MainActor
final class AppFairLibraryObserver {
    static let shared = AppFairLibraryObserver()
    private var sinks: [UInt64] = []
    private var ticker: Timer?
    private var tracking = false

    func add(_ emit: UInt64) {
        sinks.append(emit)
        if !tracking {
            tracking = true
            track()
        }
        observe_native_emit(emit, appFairEncode(snapshot()))
    }

    /// Re-arm Observation tracking over everything `snapshot()` reads, and broadcast on change.
    private func track() {
        withObservationTracking {
            _ = snapshot()
        } onChange: {
            DispatchQueue.main.async {
                AppFairLibraryObserver.shared.broadcast()
                AppFairLibraryObserver.shared.track()
            }
        }
    }

    func broadcast() {
        let event = snapshot()
        let json = appFairEncode(event)
        for sink in sinks {
            observe_native_emit(sink, json)
        }
        let busy = event.apps?.contains { $0.installing || $0.updating } ?? false
        if busy, ticker == nil {
            ticker = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
                MainActor.assumeIsolated { AppFairLibraryObserver.shared.broadcast() }
            }
        } else if !busy {
            ticker?.invalidate()
            ticker = nil
        }
    }

    func post(_ event: AppFairEvent) {
        let json = appFairEncode(event)
        for sink in sinks {
            observe_native_emit(sink, json)
        }
    }

    private func snapshot() -> AppFairEvent {
        let library = AppLibrary.current
        let apps = library.installedApps.union(library.installingApps)
        let rows = apps.map { app -> AppFairLibraryApp in
            var row = AppFairLibraryApp(id: app.id, installed: nil, installing: app.isInstalling,
                                        updating: app.isUpdating, progress: nil, error: nil)
            if app.isInstalled, let meta = app.installedMetadata {
                row.installed = .init(version: meta.version, shortVersion: meta.shortVersion,
                                      appleVersionId: meta.appleVersionID, account: meta.account)
            }
            if let progress = app.installation?.progress {
                row.progress = .init(fraction: progress.fractionCompleted,
                                     completed: progress.completedUnitCount,
                                     total: progress.totalUnitCount,
                                     cancellable: progress.isCancellable)
            }
            if #available(iOS 18.0, *), let error = app.installationError {
                row.error = error.description
            }
            return row
        }
        return AppFairEvent(type: "library", loading: library.isLoading,
                            apps: rows.sorted { $0.id < $1.id }, ids: nil, message: nil)
    }
}

// MARK: - Install verification tokens

/// The marketplace server's token exchange (docs/marketplace-server.md "Install tokens"): one
/// request names every app a tap confirms, and the server answers with a token per app it vouches
/// for and a reason for each it does not.
struct AppFairTokenRequest: Codable {
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

struct AppFairTokenResponse: Codable {
    var tokens: [String: String]?
    var failures: [String: String]?
}

enum AppFairTokens {
    static func fetch(server: String, account: String, targets: [AppFairTarget]) async throws
        -> [AppleItemID: String]
    {
        guard let url = URL(string: server.hasSuffix("/") ? "\(server)v1/install-tokens"
                                                            : "\(server)/v1/install-tokens")
        else { throw AppFairMarketError("the marketplace server address is invalid") }
        let os = ProcessInfo.processInfo.operatingSystemVersion
        let body = AppFairTokenRequest(
            account: account, platform: "iOS",
            osVersion: "\(os.majorVersion).\(os.minorVersion).\(os.patchVersion)",
            apps: targets.map {
                .init(appleItemId: String($0.appleItemId), appleVersionId: $0.appleVersionId.map(String.init),
                      bundleId: $0.bundleId, alternativeDistributionPackage: $0.packageUrl,
                      update: $0.update)
            })
        var request = URLRequest(url: url, timeoutInterval: 20)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(account, forHTTPHeaderField: "X-AppFair-Account")
        request.httpBody = try JSONEncoder().encode(body)
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            let code = (response as? HTTPURLResponse)?.statusCode ?? 0
            throw AppFairMarketError("the marketplace server answered \(code)")
        }
        let decoded = try JSONDecoder().decode(AppFairTokenResponse.self, from: data)
        var out: [AppleItemID: String] = [:]
        for (id, token) in decoded.tokens ?? [:] {
            if let item = AppleItemID(id) { out[item] = token }
        }
        if out.isEmpty, let reason = decoded.failures?.values.first {
            throw AppFairMarketError(reason)
        }
        return out
    }
}

// MARK: - The action button

/// One app a button installs or updates.
struct AppFairTarget: Codable, Sendable {
    var appleItemId: UInt64
    var appleVersionId: UInt64?
    var bundleId: String
    /// The alternative distribution package's base URL, on the marketplace's own domain.
    var packageUrl: String
    var shareUrl: String?
    var update: Bool
}

struct AppFairButtonStyle: Codable {
    var tint: String?
    var background: String?
    var fontSize: Double?
    var cornerRadius: Double?
    var borderWidth: Double?
    var borderColor: String?
    var width: Double?
    var height: Double?
}

/// The provider's params, written by `ButtonSpec` in src/lib.rs.
struct AppFairButtonSpec: Codable {
    /// `install`, `batch`, `launch`, or `delete`.
    var kind: String
    var label: String
    var server: String
    var account: String
    var targets: [AppFairTarget]
    var item: UInt64?
    var style: AppFairButtonStyle?
}

private func appFairColor(_ hex: String?) -> UIColor? {
    guard var s = hex?.trimmingCharacters(in: .whitespaces), !s.isEmpty else { return nil }
    if s.hasPrefix("#") { s.removeFirst() }
    guard let v = UInt64(s, radix: 16) else { return nil }
    let (r, g, b, a): (UInt64, UInt64, UInt64, UInt64) = s.count == 8
        ? (v >> 24 & 0xFF, v >> 16 & 0xFF, v >> 8 & 0xFF, v & 0xFF)
        : (v >> 16 & 0xFF, v >> 8 & 0xFF, v & 0xFF, 0xFF)
    return UIColor(red: CGFloat(r) / 255, green: CGFloat(g) / 255, blue: CGFloat(b) / 255,
                   alpha: CGFloat(a) / 255)
}

extension AppFairButtonSpec {
    private func metadata(_ t: AppFairTarget) -> InstallMetadata? {
        guard let url = URL(string: t.packageUrl) else { return nil }
        var m = InstallMetadata(account: account, appleItemID: t.appleItemId,
                                alternativeDistributionPackage: url, isUpdate: t.update)
        if #available(iOS 18.0, *), let share = t.shareUrl.flatMap(URL.init(string:)) {
            m.appShareURL = share
        }
        return m
    }

    /// Ask the server for every target's token, reporting each step to Rust: `confirming` when the
    /// person taps, then `confirmed` or `declined` (with the reason the install sheet shows).
    private func confirm(_ targets: [AppFairTarget]) async -> [AppleItemID: String]? {
        let ids = targets.map(\.appleItemId)
        await MainActor.run {
            AppFairLibraryObserver.shared.post(AppFairEvent(type: "confirming", ids: ids))
        }
        do {
            let tokens = try await AppFairTokens.fetch(server: server, account: account, targets: targets)
            let missing = ids.filter { tokens[$0] == nil }
            await MainActor.run {
                if !missing.isEmpty {
                    AppFairLibraryObserver.shared.post(AppFairEvent(
                        type: "declined", ids: missing,
                        message: "The marketplace did not authorize this install."))
                }
                AppFairLibraryObserver.shared.post(AppFairEvent(type: "confirmed", ids: Array(tokens.keys)))
            }
            return tokens.isEmpty ? nil : tokens
        } catch {
            let message = "\(error)"
            await MainActor.run {
                AppFairLibraryObserver.shared.post(AppFairEvent(type: "declined", ids: ids, message: message))
            }
            return nil
        }
    }

    @MainActor
    func makeButton() -> ActionButton? {
        let action: ActionButton.Action
        switch kind {
        case "launch":
            guard let item else { return nil }
            action = .launch(item)
        case "delete":
            guard let item else { return nil }
            action = .delete(item)
        case "batch":
            let installs = targets.compactMap(metadata)
            guard !installs.isEmpty else { return nil }
            let spec = self
            action = .batchInstall(BatchInstallConfiguration(installs: installs) {
                guard let tokens = await spec.confirm(spec.targets) else { return .cancel }
                return .confirmed(installVerificationTokens: tokens, authenticationContext: nil)
            })
        default:
            guard let target = targets.first, let install = metadata(target) else { return nil }
            let spec = self
            action = .install(InstallConfiguration(install: install) {
                guard let token = await spec.confirm([target])?[target.appleItemId] else { return .cancel }
                return .confirmed(installVerificationToken: token, authenticationContext: nil)
            })
        }
        let button = ActionButton(action: action)
        button.label = label
        if let s = style {
            if let c = appFairColor(s.tint) { button.tintColor = c }
            if let c = appFairColor(s.background) { button.backgroundColor = c }
            if let v = s.fontSize { button.fontSize = CGFloat(v) }
            if let v = s.cornerRadius { button.cornerRadius = CGFloat(v) }
            if let v = s.borderWidth { button.borderWidth = CGFloat(v) }
            if let c = appFairColor(s.borderColor) { button.borderColor = c }
            if let w = s.width, let h = s.height { button.size = CGSize(width: w, height: h) }
        }
        return button
    }
}

#if targetEnvironment(simulator)
extension AppFairButtonSpec {
    /// A plain button with the `ActionButton`'s label and style. Tapping it reports, through the
    /// same event an unauthorized install does, that installing needs a device.
    @MainActor
    func makeSimulatorStandIn() -> UIButton {
        var config = UIButton.Configuration.filled()
        config.title = label
        config.cornerStyle = .capsule
        config.baseForegroundColor = appFairColor(style?.tint) ?? .white
        config.baseBackgroundColor = appFairColor(style?.background) ?? .systemBlue
        let ids = kind == "launch" || kind == "delete" ? item.map { [$0] } ?? [] : targets.map(\.appleItemId)
        let button = UIButton(configuration: config, primaryAction: UIAction { _ in
            AppFairLibraryObserver.shared.post(AppFairEvent(type: "confirming", ids: ids))
            AppFairLibraryObserver.shared.post(AppFairEvent(
                type: "declined", ids: ids,
                message: "The iOS Simulator has no app marketplace. Install on a device."))
        })
        if let w = style?.width, let h = style?.height {
            button.widthAnchor.constraint(equalToConstant: CGFloat(w)).isActive = true
            button.heightAnchor.constraint(equalToConstant: CGFloat(h)).isActive = true
        }
        return button
    }
}
#endif

/// Hosts one `ActionButton`. Its action is fixed at creation, so a changed spec makes a new
/// button: the provider keys the view by its params.
struct AppFairActionButtonView: UIViewRepresentable {
    let spec: AppFairButtonSpec

    func makeUIView(context: Context) -> UIView {
        let container = UIView()
        #if targetEnvironment(simulator)
        // `ActionButton` is drawn by a system service through a synchronous XPC call the
        // simulator never answers, which would freeze the app on its first detail page. The
        // simulator gets a look-alike that explains itself, so the rest of the app (and its
        // dayscripts) still runs there.
        let button: UIView = spec.makeSimulatorStandIn()
        #else
        guard let button = spec.makeButton() else { return container }
        #endif
        button.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(button)
        NSLayoutConstraint.activate([
            button.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            button.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            button.widthAnchor.constraint(lessThanOrEqualTo: container.widthAnchor),
            button.heightAnchor.constraint(lessThanOrEqualTo: container.heightAnchor),
        ])
        return container
    }

    func updateUIView(_ uiView: UIView, context: Context) {}
}

@objc(DayView_appfair_marketplace_action)
final class AppFairActionButtonProvider: DaySwiftUIProvider {
    override func body(_ params: String?) -> AnyView {
        guard let params, let spec = try? appFairDecode(AppFairButtonSpec.self, params) else {
            return DaySwiftUI.errorView("appfair_marketplace_action")
        }
        return AnyView(AppFairActionButtonView(spec: spec).id(params))
    }
}
