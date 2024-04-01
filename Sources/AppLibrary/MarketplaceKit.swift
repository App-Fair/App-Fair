// This is free software: you can redistribute and/or modify it
// under the terms of the GNU General Public License 3.0
// as published by the Free Software Foundation https://fsf.org
import Foundation
import Observation

public enum AppDistributor {

    public static var current: AppDistributor = .marketplace("App Fair")

    case appStore

    case testFlight

    case marketplace(String)

    case other
}

@MainActor final public class AppLibrary {

    public static let current: AppLibrary = AppLibrary()

    @MainActor final public var installedApps: Set<AppLibrary.App> = []

    @MainActor final public var installingApps: Set<AppLibrary.App> = []

    @MainActor final public var isLoading: Bool = false

    @MainActor final public func app(forAppleItemID appleItemID: AppleItemID) -> AppLibrary.App {
        fatalError("TODO")
    }

    nonisolated final public func requestAppInstallation(for url: URL, account: String, installVerificationToken: String) async throws {
        fatalError("TODO")
    }

    nonisolated final public func requestAppInstallationFromBrowser(for url: URL, referrer: URL) async throws {
        fatalError("TODO")
    }

    nonisolated final public func requestAppUpdate(for url: URL, account: String, installVerificationToken: String) async throws {
        fatalError("TODO")
    }

    nonisolated final public var searchTerritory: String? {
        fatalError("TODO")
    }

    nonisolated final public func setSearchTerritory(_ territory: String?) async {
        fatalError("TODO")
    }

    nonisolated final public func requestLicenseRenewal(appleItemIDs: [UInt64]) async throws {
        fatalError("TODO")
    }
}

extension AppLibrary : Observable {
}

extension AppLibrary {

    @MainActor final public class App : Identifiable, Hashable {

        /// The stable identity of the entity associated with this instance.
        nonisolated final public let id: AppleItemID

        init(id: AppleItemID) {
            self.id = id
        }

        public struct Metadata : Sendable, Equatable {

            public let appleVersionID: AppleVersionID

            public let version: String

            public let shortVersion: String

            public let account: String?

            /// Returns a Boolean value indicating whether two values are equal.
            ///
            /// Equality is the inverse of inequality. For any values `a` and `b`,
            /// `a == b` implies that `a != b` is `false`.
            ///
            /// - Parameters:
            ///   - lhs: A value to compare.
            ///   - rhs: Another value to compare.
            public static func == (a: AppLibrary.App.Metadata, b: AppLibrary.App.Metadata) -> Bool {
                fatalError("TODO")
            }
        }

        public struct Installation : Sendable {

            /// The progress representing the download & installation of this app. It may be used to pause, resume, or cancel installation depending on the state of this object.
            public var progress: Progress  {
                fatalError("TODO")
            }
        }

        @MainActor final public var installedMetadata: AppLibrary.App.Metadata?  {
            fatalError("TODO")
        }

        @MainActor final public var installation: AppLibrary.App.Installation?  {
            fatalError("TODO")
        }

        @MainActor final public var isInstalled: Bool  {
            fatalError("TODO")
        }

        @MainActor final public var isInstalling: Bool  {
            fatalError("TODO")
        }

        @MainActor final public var isUpdating: Bool  {
            fatalError("TODO")
        }

        /// Returns a Boolean value indicating whether two values are equal.
        ///
        /// Equality is the inverse of inequality. For any values `a` and `b`,
        /// `a == b` implies that `a != b` is `false`.
        ///
        /// - Parameters:
        ///   - lhs: A value to compare.
        ///   - rhs: Another value to compare.
        nonisolated public static func == (lhs: AppLibrary.App, rhs: AppLibrary.App) -> Bool  {
            fatalError("TODO")
        }

        /// Hashes the essential components of this value by feeding them into the
        /// given hasher.
        ///
        /// Implement this method to conform to the `Hashable` protocol. The
        /// components used for hashing must be the same as the components compared
        /// in your type's `==` operator implementation. Call `hasher.combine(_:)`
        /// with each of these components.
        ///
        /// - Important: In your implementation of `hash(into:)`,
        ///   don't call `finalize()` on the `hasher` instance provided,
        ///   or replace it with a different instance.
        ///   Doing so may become a compile-time error in the future.
        ///
        /// - Parameter hasher: The hasher to use when combining the components
        ///   of this instance.
        nonisolated final public func hash(into hasher: inout Hasher)  {
            fatalError("TODO")
        }

        /// A type representing the stable identity of the entity associated with
        /// an instance.
        public typealias ID = AppleItemID

        /// The hash value.
        ///
        /// Hash values are not guaranteed to be equal across different executions of
        /// your program. Do not save hash values to use during a future execution.
        ///
        /// - Important: `hashValue` is deprecated as a `Hashable` requirement. To
        ///   conform to `Hashable`, implement the `hash(into:)` requirement instead.
        ///   The compiler provides an implementation for `hashValue` for you.
        nonisolated final public var hashValue: Int  {
            fatalError("TODO")
        }
    }
}

extension AppLibrary : Sendable {
}

extension AppLibrary.App : Observable {
}

extension AppLibrary.App : Sendable {
}

public struct AppVersion : Sendable, CustomStringConvertible {

    public let appleItemID: AppleItemID

    public let appleVersionID: UInt64

    public init(appleItemID: AppleItemID, appleVersionID: UInt64)  {
        fatalError("TODO")
    }

    /// A textual representation of this instance.
    ///
    /// Calling this property directly is discouraged. Instead, convert an
    /// instance of any type to a string by using the `String(describing:)`
    /// initializer. This initializer works with any type, and uses the custom
    /// `description` property for types that conform to
    /// `CustomStringConvertible`:
    ///
    ///     struct Point: CustomStringConvertible {
    ///         let x: Int, y: Int
    ///
    ///         var description: String {
    ///             return "(\(x), \(y))"
    ///         }
    ///     }
    ///
    ///     let p = Point(x: 21, y: 30)
    ///     let s = String(describing: p)
    ///     print(s)
    ///     // Prints "(21, 30)"
    ///
    /// The conversion of `p` to a string in the assignment to `s` uses the
    /// `Point` type's `description` property.
    public var description: String  {
        fatalError("TODO")
    }
}

public typealias AppleItemID = UInt64

public typealias AppleVersionID = UInt64

public struct AutomaticUpdate : Sendable {

    public let appleItemID: AppleItemID

    public let alternativeDistributionPackage: URL

    public let account: String

    public let installVerificationToken: String

    public init(appleItemID: AppleItemID, alternativeDistributionPackage: URL, account: String, installVerificationToken: String) {
        fatalError("TODO")
    }
}

public struct InstallRequirements : Sendable, Codable {

    public var minimumSystemVersion: String?

    public var requiredDeviceCapabilities: Set<String>?

    public var ageRatingRank: Int?

    public var expectedInstallSize: UInt64?

    public init() {
    }

    public func satisfiedByDevice() -> Bool {
        fatalError("TODO")
    }

    /// Encodes this value into the given encoder.
    ///
    /// If the value fails to encode anything, `encoder` will encode an empty
    /// keyed container in its place.
    ///
    /// This function throws an error if any values are invalid for the given
    /// encoder's format.
    ///
    /// - Parameter encoder: The encoder to write data to.
    public func encode(to encoder: any Encoder) throws {
        fatalError("TODO")
    }

    /// Creates a new instance by decoding from the given decoder.
    ///
    /// This initializer throws an error if reading from the decoder fails, or
    /// if the data read is corrupted or otherwise invalid.
    ///
    /// - Parameter decoder: The decoder to read data from.
    public init(from decoder: any Decoder) throws {
        fatalError("TODO")
    }
}

//public protocol MarketplaceExtension : AppExtension, Sendable {
//
//    func additionalHeaders(for request: URLRequest, account: String) -> [String : String]?
//
//    func availableAppVersions(forAppleItemIDs ids: [AppleItemID]) -> [AppVersion]?
//
//    func requestFailed(with response: HTTPURLResponse) -> Bool
//
//    func automaticUpdates(for installedAppVersions: [AppVersion]) async throws -> [AutomaticUpdate]
//}
//
//extension MarketplaceExtension {
//
//    /// The configuration object for this app extension.
//    ///
//    /// For any app extension that you create, provide a computed `configuration` property that
//    /// defines your app extension's configuration.
//    ///
//    /// Swift infers the app extension's  associated type based on the configuration
//    /// provided by the `configuration` property.
//    public var configuration: some MarketplaceExtensionConfiguration  {
//        fatalError("TODO")
//    }
//}
//
//public protocol MarketplaceExtensionConfiguration : AppExtensionConfiguration {
//}

public enum MarketplaceKitError : Error, CustomStringConvertible, Sendable, Codable {

    /// Failure due to an unknown error.
    case unknown

    /// The requested install requires capabilities not available on this device.
    case missingCapabilities([String])

    /// The requested install does not run on this device's platform.
    case unsupportedPlatform

    /// The requested install requires a minimum platform version that is greater than this device.
    case minimumPlatformVersionNotSatisfied(String)

    /// Installations are restricted on this device.
    case installationRestricted

    /// Installations of marketplaces are denied on this device.
    case installationOfMarketplaceDenied

    /// The requested install has a rating that exceeds this device's restrictions.
    case ratingRestricted

    /// The requested install requires more storage space than the device has available.
    case insufficientStorageSpace(Measurement<UnitInformationStorage>)

    /// The requested install has no supported variant for this device.
    case noSupportedVariant

    /// The requested operation cannot be completed because the app isn't installed
    case appNotInstalled

    /// The manifest is invalid, or cannot be read
    case invalidManifest

    /// A network error occurred during the request
    case networkError

    /// Invalid URL for an Alternative Distribution Package
    case invalidAlternativeDistributionPackageURL

    /// The signature of the Alternative Distribution Package wasn't available or was invalid
    case invalidAlternativeDistributionPackageSignature

    /// The requested feature is unavailable
    case featureUnavailable

    /// The app isn't eligible to be installed
    case cancelled

    /// The provided install type doesn't match the install that would occur
    case mismatchedInstallType

    /// An error fetching an OAuth Token
    case oauthTokenError

    /// No valid license provided
    case invalidLicense

    /// An invalid URL was provided
    case invalidURL

    /// The required install verification token is missing
    case missingInstallVerificationToken

    /// A textual representation of this instance.
    ///
    /// Calling this property directly is discouraged. Instead, convert an
    /// instance of any type to a string by using the `String(describing:)`
    /// initializer. This initializer works with any type, and uses the custom
    /// `description` property for types that conform to
    /// `CustomStringConvertible`:
    ///
    ///     struct Point: CustomStringConvertible {
    ///         let x: Int, y: Int
    ///
    ///         var description: String {
    ///             return "(\(x), \(y))"
    ///         }
    ///     }
    ///
    ///     let p = Point(x: 21, y: 30)
    ///     let s = String(describing: p)
    ///     print(s)
    ///     // Prints "(21, 30)"
    ///
    /// The conversion of `p` to a string in the assignment to `s` uses the
    /// `Point` type's `description` property.
    public var description: String  {
        fatalError("TODO")
    }

    /// Encodes this value into the given encoder.
    ///
    /// If the value fails to encode anything, `encoder` will encode an empty
    /// keyed container in its place.
    ///
    /// This function throws an error if any values are invalid for the given
    /// encoder's format.
    ///
    /// - Parameter encoder: The encoder to write data to.
    public func encode(to encoder: any Encoder) throws  {
        fatalError("TODO")
    }

    /// Creates a new instance by decoding from the given decoder.
    ///
    /// This initializer throws an error if reading from the decoder fails, or
    /// if the data read is corrupted or otherwise invalid.
    ///
    /// - Parameter decoder: The decoder to read data from.
    public init(from decoder: any Decoder) throws  {
        fatalError("TODO")
    }
}

/// The URI scheme used to indicate an installation link
public let MarketplaceKitURIScheme: String = "marketplace://"

/// The URI scheme used to indicate an installation link
//public var MarketplaceKitURIScheme: String = ""

