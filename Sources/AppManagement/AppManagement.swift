// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import Foundation
import AppLibrary

/// A type representing a library of ManagedApps
public final class ManagedAppLibrary : Sendable {
    public static let currentDistributor: ManagedAppLibrary = ManagedAppLibrary()

    /// The current managed apps available to this device.
    ///
    /// The current managed apps are returned as a Result<[ManagedApp], ManagedAppDistributionError>. Use an asynchronous for loop to update your views when the current managed apps change. Fetching the list of managed apps may fail with ManagedAppDistributionError.networkError if the device cannot retrieve the metadata for the apps, for example if the device is offline.
    public var availableApps: ManagedApps {
        ManagedApps()
    }

    private init() {
    }

    /// The array of managed apps as an asynchronous sequence that is updated when managed apps become available or unavailable
    ///
    /// Also known as: `AsyncManagedAppsSequence`
    public struct ManagedApps : AsyncSequence {
        public typealias Element = AsyncIterator.Element

        public func makeAsyncIterator() -> AsyncIterator {
            AsyncIterator()
        }

        /// The iterator for ManagedAppLibrary.ManagedApps
        public struct AsyncIterator : AsyncIteratorProtocol {
            public typealias Element = Result<[ManagedApp], ManagedAppDistributionError>

            public func next() async throws -> Element? {
                nil // TODO
            }
        }
    }
}

public enum ManagedAppDistributionError : Error {
    case networkError
}
