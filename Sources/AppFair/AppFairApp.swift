// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import Foundation
import OSLog
import SwiftUI

let logger: Logger = Logger(subsystem: "org.appfair.App-Fair", category: "AppFair")

/// The Android SDK number we are running against, or `nil` if not running on Android
let androidSDK = ProcessInfo.processInfo.environment["android.os.Build.VERSION.SDK_INT"].flatMap({ Int($0) })

/// The shared top-level view for the app, loaded from the platform-specific App delegates below.
///
/// The default implementation merely loads the `ContentView` for the app and logs a message.
@available(iOS 17.2, *)
public struct RootView : View {
    public init() {
    }

    public var body: some View {
        ContentView()
            .task {
                logger.log("Welcome to Skip on \(androidSDK != nil ? "Android" : "iOS")!")
                logger.warning("Skip app logs are viewable in the Xcode console for iOS; Android logs can be viewed in Studio or using adb logcat")
            }
    }
}

#if !SKIP
public protocol AppFairApp : App {
}

/// The entry point to the AppFair app.
/// The concrete implementation is in the AppFairApp module.
@available(iOS 17.2, *)
public extension AppFairApp {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
    }
}
#endif
