// This is free software: you can redistribute and/or modify it
// under the terms of the GNU Lesser General Public License 3.0
// as published by the Free Software Foundation https://fsf.org

import Foundation
import OSLog
import Observation
import SkipSQL

let logger: Logger = Logger(subsystem: "app.libary", category: "AppLibrary")

/// A type representing an app.
public struct ManagedApp : Identifiable, Sendable, Encodable {
    /// The stable identity of the entity associated with this instance.
    public let id: String

    /// The localized name of the app.
    public let name: String

    /// The localized subtitle of the app.
    public let subtitle: String?

    /// The description of the app
    //public let description: String?

    /// A URL for the icon of the app. The icon will scale to fit the given size.
    public func iconURL(fitting: CGSize) -> URL? {
        fatalError("TODO")
    }

    /// An Array of URLs for the screenshots of the app. The screenshots will scale to fit the given size.
    public func screenshotURLs(fitting: CGSize) -> [URL] {
        fatalError("TODO")
    }

    /// The platform of the app
    public let platform: Platform

    /// The operating system compatibility requirements for the app
    public let requirements: String?

    /// The languages supported by the app
    //public let languages: [Locale.Language]

    /// The language of app metadata
    //public let metadataLanguage: Locale.Language?

    /// The URL of app developer’s website
    public let developerWebsite: URL?

    /// The genres of the app
    public let genres: [String]

    /// The age rating for the content of the app
    public let contentRating: String?

    /// The URL of app’s privacy policy
    public let privacyPolicy: URL?

    /// The copyright of the app
    public let copyright: String?

    /// The URL of app’s license agreement
    public var licenseAgreement: URL?

    /// The version of the app
    public let version: String?

    /// The localized developer release notes version of the app.
    public let releaseNotes: String?

    /// The release date version of the app
    public let releaseDate: Date?

    /// The size of the app in bytes
    public let fileSize: UInt64?

    /// A value representing the platform for a ManagedApp
    public struct Platform : Hashable, Sendable, Codable {
        public let identifier: String

        /// The Platform representing iOS
        public static let iOS: ManagedApp.Platform = Platform(identifier: "ios")

        /// The Platform representing macOS
        public static var macOS: ManagedApp.Platform = Platform(identifier: "macos")

        /// A textual representation of this instance.
        public var description: String { identifier }
    }

//    public enum CodingKeys : String, CodingKey, CaseIterable {
//        case id
//        case name
//        case subtitle
//        case description
//        case platform
//        case requirements
//        case developerWebsite
//        case genres
//        case contentRating
//        case privacyPolicy
//        case copyright
//        case licenseAgreement
//        case version
//        case releaseNotes
//        case releaseDate
//        case fileSize
//    }

}


//extension ManagedApp {
//    public static let tableName = "APP"
//
//    public static let columns = ManagedApp.CodingKeys.allCases.map(\.columnName)
//
//    public static var columnNames: String {
//        columns.joined(separator: ", ")
//    }
//
//    public static var columnUpdate: String {
//        columns.joined(separator: " = ?, ") + " = ?"
//    }
//
//    /// The SQL insert statement for a row representing this item
//    public static var insertSQL: String {
//        "INSERT INTO \(QTBL) (\(columnNames)) VALUES (\(columns.map({ _ in "?" }).joined(separator: ", ")))"
//    }
//
//    /// Initializes from a result set that was selected from `columnNames` from the `fromIndex` offset.
//    init(fromCursor statement: SQLStatement, fromIndex: Int32 = 0) throws {
//        func fail<T>(_ msg: String) throws -> T {
//            throw MissingColumnError(errorDescription: "Could not load DataItem column: \(msg)")
//        }
//
//        self.id = try statement.value(at: fromIndex + 0).integerValue ?? fail("id")
//        self.title = try statement.value(at: fromIndex + 1).textValue ?? fail("title")
//        self.created = try statement.value(at: fromIndex + 2).floatValue.flatMap({ Date(timeIntervalSince1970: $0) }) ?? fail("created")
//        self.modified = statement.value(at: fromIndex + 3).floatValue.flatMap({ Date(timeIntervalSince1970: $0) })
//        self.contents = try statement.value(at: fromIndex + 4).textValue ?? fail("contents")
//        self.rating = statement.value(at: fromIndex + 5).floatValue
//        self.thumbnail = statement.value(at: fromIndex + 6).blobValue
//    }
//
//    /// The SQLValue array for this item in the same order as `columnNames`
//    var itemParameters: [SQLValue] {
//        [
//            id.flatMap({ SQLValue.integer($0) }) ?? SQLValue.null,
//            .text(title),
//            .float(created.timeIntervalSince1970),
//            modified.flatMap({ SQLValue.float($0.timeIntervalSince1970) }) ?? SQLValue.null,
//            .text(contents),
//            rating.flatMap({ SQLValue.float($0) }) ?? SQLValue.null,
//            thumbnail.flatMap({ SQLValue.blob($0) }) ?? SQLValue.null,
//        ]
//    }
////}
//
//// the quoted table name for the DataItem
//let QTBL = "\"\(ManagedApp.tableName)\""
//
//public class AppLibrary : ObservableObject {
//    var ctx: SQLContext
//
//    /// A reusable statement for inserting new DataItem instance rows
//    private var insertStatement: SQLStatement?
//
//    /// A reusable statement for selecting a DataItem by the rowid
//    private var selectByIDStatement: SQLStatement?
//
//    /// This needs to be set when manually editing rows from the update hook so as to not trigger cyclic updates
//    private var updateFromHook: Bool = false
//
//    /// A list of all the rowids for all the `DataItem` instances in the database.
//    ///
//    /// This property is updated whenever a row is added or delete from the database, by use of the `onUpdate` hook.
//    @Published public var rowids: [DataItem.RowID] = [] {
//        willSet {
//            handleRowIdChange(newValue: newValue)
//        }
//    }
//
//    /// Creates a new database manager with the given persistent database path, or `nil`
//    public init(url: URL?) throws {
//        self.ctx = try SQLContext(path: url?.path ?? ":memory:", flags: [.readWrite, .create], logLevel: .info)
//
//        try migrateSchema()
//        try reload()
//    }
//
//    /// Creates the initial database schema and performs any migrations for the schema version integer stored in the `DB_SCHEMA_VERSION` table.
//    private func migrateSchema() throws {
//        let startTime = Date.now
//
//        // track the version of the schema in the database, which can be used for schema migration
//        try ctx.exec(sql: "CREATE TABLE IF NOT EXISTS DB_SCHEMA_VERSION (id INTEGER PRIMARY KEY, version INTEGER)")
//        try ctx.exec(sql: "INSERT OR IGNORE INTO DB_SCHEMA_VERSION (id, version) VALUES (0, 0)")
//        var currentVersion = try ctx.query(sql: "SELECT version FROM DB_SCHEMA_VERSION").first?.first?.integerValue ?? 0
//
//        func migrateSchema(v version: Int64, ddl: String) throws {
//            if currentVersion < version {
//                let startTime = Date.now
//                try ctx.exec(sql: ddl) // perform the DDL operation
//                // then update the schema version
//                try ctx.exec(sql: "UPDATE DB_SCHEMA_VERSION SET version = ?", parameters: [SQLValue.integer(version)])
//                currentVersion = version
//                logger.log("updated database schema to \(version) in \(startTime.durationToNow)")
//            }
//        }
//
//
//        // the initial creation script for a new database
//        try migrateSchema(v: 1, ddl: """
//        CREATE TABLE IF NOT EXISTS \(QTBL) (\(DataItem.CodingKeys.id.rawValue) INTEGER PRIMARY KEY AUTOINCREMENT)
//        """)
//
//        // incrementally migrate up to the current schema version
//        func addDataItemColumn(_ key: DataItem.CodingKeys) -> String {
//            "ALTER TABLE \(QTBL) ADD COLUMN \(key.rawValue) \(key.ddl)"
//        }
//
//        try migrateSchema(v: 2, ddl: addDataItemColumn(.title))
//        try migrateSchema(v: 3, ddl: addDataItemColumn(.created))
//        try migrateSchema(v: 4, ddl: addDataItemColumn(.modified))
//        try migrateSchema(v: 5, ddl: addDataItemColumn(.contents))
//        try migrateSchema(v: 6, ddl: addDataItemColumn(.rating))
//        try migrateSchema(v: 7, ddl: addDataItemColumn(.thumbnail))
//        // future migrations to follow…
//
//        logger.log("initialized database in \(startTime.durationToNow)")
//
//        // install an update hook to always keep the local list of ids in sync with the database
//        ctx.onUpdate(hook: databaseUpdated)
//    }
//
//    /// Invoked from the `onUpdate` hook whenever a ROWID table changes in the database
//    private func databaseUpdated(action: SQLAction, rowid: Int64, dbname: String, tblname: String) {
//        updateFromHook = true
//        defer { updateFromHook = false }
//        //logger.debug("databaseUpdated: \(action.description) \(dbname).\(tblname) \(rowid)")
//        if tblname == DataItem.tableName {
//            switch action {
//            case .insert: self.rowids.append(rowid)
//            case .delete: self.rowids.removeAll(where: { $0 == rowid })
//            case .update: self.rowids = self.rowids
//            }
//        }
//    }
//
//    private func handleRowIdChange(newValue: [DataItem.RowID]) {
//        //logger.debug("handleRowIdChange: \(self.rowids.count) -> \(newValue.count)")
//        // this property is updated from the SwiftUI list, and so we need to detect when the user deletes a list item and issue the delete statement
//        if !updateFromHook, self.rowids.count != newValue.count {
//            let oldids = Set(self.rowids)
//            let newids = Set(newValue)
//            let deleteRows = oldids.subtracting(newids)
//            if !deleteRows.isEmpty {
//                attempt {
//                    try delete(ids: Array(deleteRows))
//                }
//            }
//        }
//    }
//
//    /// Attempts the given operation and log an error if it fails
//    public func attempt(block: () throws -> ()) {
//        do {
//            try block()
//        } catch {
//            logger.warning("attempt failure: \(error)")
//        }
//    }
//
//    /// Reload all the records from the underlying table
//    public func reload(orderBy: String? = nil) throws {
//        let newIds = try self.queryIDs(orderBy: orderBy ?? "ROWID DESC")
//        self.rowids = newIds
//    }
//
//    /// Deletes the items with the given IDs.
//    public func delete(ids: [Int64]) throws {
//        return try ctx.exec(sql: "DELETE FROM \(QTBL) WHERE ID IN (\(ids.map(\.description).joined(separator: ",")))")
//    }
//
//    /// Updates the items in the database based on their corresponding IDs.
//    public func update(items: [DataItem]) throws {
//        let sql = "UPDATE \(QTBL) SET " + DataItem.columnUpdate + " WHERE id = ?"
//        let stmnt = try ctx.prepare(sql: sql)
//        defer { stmnt.close() }
//        for item in items {
//            if let id = item.id {
//                try stmnt.update(parameters: item.itemParameters + [SQLValue.integer(id)])
//            }
//        }
//    }
//
//    /// Prepares the given SQL statement, caching and re-using it into the given statement handle.
//    func prepare(sql: String, into statement: inout SQLStatement?) throws -> SQLStatement {
//        if let stmnt = statement {
//            #if SKIP
//            return stmnt! // https://github.com/skiptools/skip/issues/50
//            #else
//            return stmnt // already cached
//            #endif
//        } else {
//            let stmnt = try ctx.prepare(sql: sql)
//            statement = stmnt // save to the cache
//            return stmnt
//        }
//    }
//
//    /// Insert new instances of the item into the database.
//    @discardableResult public func insert(items: [DataItem]) throws -> [DataItem.RowID] {
//        return try ctx.transaction {
//            var ids: [DataItem.RowID] = []
//            for item in items {
//                try prepare(sql: DataItem.insertSQL, into: &insertStatement)
//                    .update(parameters: item.itemParameters)
//                ids.append(ctx.lastInsertRowID)
//            }
//            return ids
//        }
//    }
//
//    /// Returns the total number of rows for a table.
//    public func queryDataItemCount() throws -> Int64? {
//        try ctx.prepare(sql: "SELECT COUNT(*) FROM \(QTBL)").nextValues(close: true)?.first?.integerValue
//    }
//
//    /// Fetches the items with the corresponding IDs.
//    ///
//    /// Note that the order of the returned items is unrelated to the order of the `ids` parameter, nor is the returned array guaranteed to contains all the identifiers, since missing elements are not detected nor considered an error.
//    public func fetch(ids: [Int64]) throws -> [DataItem] {
//        if ids.isEmpty {
//            return []
//        } else {
//            let sql = "\(DataItem.CodingKeys.id.rawValue) IN (" + ids.map({ _ in "?" }).joined(separator: ",") + ")"
//            return try queryDataItems(where: sql, ids.map({ SQLValue.integer($0) }), cache: ids.count == 1)
//        }
//    }
//
//    public func queryDataItems(where whereClause: String? = nil, _ values: [SQLValue] = [], cache: Bool = false) throws -> [DataItem] {
//        var sql = "SELECT \(DataItem.columnNames) FROM \(QTBL)"
//        if let whereClause = whereClause {
//            sql += " WHERE " + whereClause
//        }
//        let stmnt = try cache == true
//            ? prepare(sql: sql, into: &selectByIDStatement)
//            : ctx.prepare(sql: sql)
//        try stmnt.bind(parameters: values)
//        defer {
//            if cache {
//                stmnt.reset()
//            } else {
//                stmnt.close()
//            }
//        }
//        var items: [DataItem] = []
//        while try stmnt.next() {
//            items.append(try DataItem(fromCursor: stmnt))
//        }
//
//        return items
//    }
//
//    public func queryIDs(limit: Int64? = nil, where whereClause: String? = nil, _ values: [SQLValue] = [], orderBy: String? = nil) throws -> [Int64] {
//        let startTime = Date.now
//        var sql = "SELECT \(DataItem.CodingKeys.id.rawValue) FROM \(QTBL)"
//        if let whereClause = whereClause {
//            sql += " WHERE " + whereClause
//        }
//        if let orderBy = orderBy {
//            sql += " ORDER BY " + orderBy
//        }
//        let stmnt = try ctx.prepare(sql: sql)
//        try stmnt.bind(parameters: values)
//        defer { stmnt.close() }
//        var ids: [Int64] = []
//        while try stmnt.next() {
//            ids.append(stmnt.integer(at: 0))
//        }
//
//        // iOS Sim: 1000 rows in 0.001
//        // Android emulator: 1000 rows in 0.005
//        logger.log("fetched \(ids.count) in \(startTime.durationToNow)")
//        return ids
//    }
//}
//
//public struct MissingColumnError : LocalizedError {
//    public var errorDescription: String?
//}
//
//
//extension Date {
//    /// The duration of this Date from now, rounded to the millisecond
//    public var durationToNow: Double {
//        round(Date.now.timeIntervalSince(self) * 1_000.0) / 1_000.0
//    }
//}
