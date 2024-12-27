// This is free software: you can redistribute and/or modify it
// under the terms of the GNU General Public License 3.0
// as published by the Free Software Foundation https://fsf.org
import Foundation
import OSLog
import Observation
import SkipSQL

@Observable public final class AppSource {
    public static let shared = AppSource()

    public static let rawContentURL = URL(string: "https://raw.githubusercontent.com")!

    public var apps: [AppSource.App] = [
        App(id: "skiptools/skipapp-showcase", name: "Showcase"),
    ]

    public struct App : Identifiable, Hashable {
        public let id: String
        public let name: String

        public var repositoryURL: URL {
            URL(string: "https://github.com/\(id)")!
        }

        /// The base raw content URL for the given tag, or `main` if unspecified
        func contentURL(version: String?) -> URL {
            let ref = version != nil ? "tags/\(version!)" : "heads/main"
            return AppSource.rawContentURL
                .appendingPathComponent(id)
                .appendingPathComponent("refs")
                .appendingPathComponent(ref)
        }

        func releaseURL(version: String?) -> URL {
            let ref = version != nil ? "tag/\(version!)" : "latest"

            return repositoryURL
                .appendingPathComponent(id)
                .appendingPathComponent("releases")
                .appendingPathComponent(ref)
        }

        func installURL(version: String?) -> URL {
            releaseURL(version: version)
                .appendingPathComponent("\(name)-release")
                .appendingPathExtension(isAndroid ? "apk" : "ipa")
        }

        func fastlaneURL(version: String?, locale: String?) -> URL {
            contentURL(version: version)
                .appendingPathComponent("Darwin/fastlane/metadata")
                .appendingPathComponent(locale ?? "en-US")
        }

        public func iconURL(version: String?) -> URL {
            contentURL(version: version)
                .appendingPathComponent("Darwin/Assets.xcassets/AppIcon.appiconset/AppIcon-29.png")
        }

        public func subtitleURL(version: String? = nil, locale: String? = nil) -> URL {
            fastlaneURL(version: version, locale: locale)
                .appendingPathComponent("subtitle.txt")
        }

        public func descriptionURL(version: String? = nil, locale: String? = nil) -> URL {
            fastlaneURL(version: version, locale: locale)
                .appendingPathComponent("description.txt")
        }
    }

    // https://developer.android.com/reference/android/content/pm/PackageInstaller.html

    /*
    private fun installApk(uri: Uri) {
        val packageInstaller = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
        val sessionId = packageInstaller.createSession(params)
        val session = packageInstaller.openSession(sessionId)

        val inputStream = context.contentResolver.openInputStream(uri)
        val outputStream = session.openWrite("app.apk", 0, -1)

        inputStream?.use { input ->
            outputStream.use { output ->
                input.copyTo(output)
            }
        }

        session.fsync(outputStream)
        inputStream?.close()
        outputStream.close()

        val intent = Intent(context, context.javaClass)
        val pendingIntent = PendingIntent.getActivity(context, 0, intent, 0)
        session.commit(pendingIntent.intentSender)
    }
     */
}

@MainActor @Observable public final class ViewModel {
    public init() {
    }
}

let isJava = ProcessInfo.processInfo.environment["java.io.tmpdir"] != nil
/// True when running within an Android environment (either an emulator or device)
let isAndroid = isJava && ProcessInfo.processInfo.environment["ANDROID_ROOT"] != nil
