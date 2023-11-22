// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import XCTest
import OSLog
import Foundation
@testable import AppManagement

let logger: Logger = Logger(subsystem: "AppManagement", category: "Tests")

@available(macOS 13, *)
final class AppManagementTests: XCTestCase {
    func testAppManagement() throws {
        logger.log("running testAppManagement")
        XCTAssertEqual(1 + 2, 3, "basic test")
        
        // load the TestData.json file from the Resources folder and decode it into a struct
        let resourceURL: URL = try XCTUnwrap(Bundle.module.url(forResource: "TestData", withExtension: "json"))
        let testData = try JSONDecoder().decode(TestData.self, from: Data(contentsOf: resourceURL))
        XCTAssertEqual("AppManagement", testData.testModuleName)
    }
}

struct TestData : Codable, Hashable {
    var testModuleName: String
}