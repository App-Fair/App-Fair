// SPDX-License-Identifier: GPL-3.0-only
import XCTest
import OSLog
import Foundation
@testable import AppFair

let logger: Logger = Logger(subsystem: "AppFair", category: "Tests")

@available(macOS 13, *)
final class AppFairTests: XCTestCase {
    func testAppFair() throws {
        logger.log("running testAppFair")
        XCTAssertEqual(1 + 2, 3, "basic test")
        
        // load the TestData.json file from the Resources folder and decode it into a struct
        let resourceURL: URL = try XCTUnwrap(Bundle.module.url(forResource: "TestData", withExtension: "json"))
        let testData = try JSONDecoder().decode(TestData.self, from: Data(contentsOf: resourceURL))
        XCTAssertEqual("AppFair", testData.testModuleName)
    }
}

struct TestData : Codable, Hashable {
    var testModuleName: String
}
