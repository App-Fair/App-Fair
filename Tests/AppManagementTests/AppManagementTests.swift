// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import XCTest
import OSLog
import Foundation
import SkipModel
import AppLibrary
@testable import AppManagement

let logger: Logger = Logger(subsystem: "AppManagement", category: "Tests")

@available(macOS 13, *)
final class AppManagementTests: XCTestCase {
    func testAppManagement() async throws {
        let viewModel = ViewModel()
        await viewModel.getApps()
    }
}

// Define a model that obtains a list of managed apps.
final class ViewModel : ObservableObject {
    @Published var content: [Content] = []

    enum Content {
        case managedApp(ManagedApp)
        case developerContent(title: String, action: () -> Void)
    }


    func getApps() async {
        do {
            for try await result in ManagedAppLibrary.currentDistributor.availableApps {
                content = try result.get().map({ Content.managedApp($0) })
            }
        } catch {
            // Handle errors here.
        }
    }
}
