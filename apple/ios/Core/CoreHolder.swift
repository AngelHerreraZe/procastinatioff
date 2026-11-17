// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import Security

/// Single access point to the Rust cores (app and extensions). They all share the iPhone's keys:
/// - core.db: this iPhone's profile (its own, or the minor's if it is their iPhone).
/// - contact.db / watching.db: trusted contact channels (mine, and of the person I support).
/// - family-<id>.db: (guardian) one profile per minor I manage. They never block THIS iPhone.
/// The databases live in the App Group with the default protection (`completeUntilFirstUserAuthentication`):
/// background refresh opens them at night with the iPhone locked.
enum CoreHolder {
    private static let lock = NSLock()
    private static var cores: [String: Core] = [:]

    private static var keychainGroup: String {
        (Bundle.main.object(forInfoDictionaryKey: "AppIdentifierPrefix") as? String ?? "") + "dev.procastinacioff.shared"
    }

    static func open(_ file: String) throws -> Core {
        lock.lock()
        defer { lock.unlock() }
        if let c = cores[file] { return c }
        guard let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppGroup.id) else {
            throw CoreError.Storage(msg: "no App Group")
        }
        let c = try Core.open(dbPath: dir.appendingPathComponent(file).path, secrets: KeychainSecretStore(accessGroup: keychainGroup))
        cores[file] = c
        return c
    }

    static func shared() throws -> Core { try open("core.db") }
    static func contactChannel() throws -> Core { try open("contact.db") }
    static func watchingBox() throws -> Core { try open("watching.db") }

    /// The mailbox only counts if it was created or joined (an empty database is never opened out of curiosity).
    static func existing(_ file: String) -> Core? {
        guard let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppGroup.id),
              FileManager.default.fileExists(atPath: dir.appendingPathComponent(file).path),
              let c = try? open(file), !c.members().isEmpty else { return nil }
        return c
    }

    static func family(_ id: String) throws -> Core {
        guard id.range(of: "^[0-9a-f]{16}$", options: .regularExpression) != nil else {
            throw CoreError.Protocol(msg: "invalid profile id")
        }
        return try open("family-\(id).db")
    }

    static func clock() -> Clock { CoreClock.now(try? shared()) }
}

/// (Guardian) Minors' profiles: id → name. Only on this iPhone; the name never leaves it.
enum FamilyProfiles {
    private static let key = "family-profiles"
    private static var defaults: UserDefaults? { UserDefaults(suiteName: AppGroup.id) }

    static func all() -> [String: String] { defaults?.dictionary(forKey: key) as? [String: String] ?? [:] }

    static func newId() -> String {
        var bytes = [UInt8](repeating: 0, count: 8)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// Registered only once the profile exists in the core.
    static func save(id: String, name: String) {
        var all = all()
        all[id] = String(name.prefix(64))
        defaults?.set(all, forKey: key)
    }
}

enum Texts {
    static func describe(_ what: RequestKind) -> String {
        switch what {
        case .extraPermit(let minutes): return String(localized: "\(Int(minutes)) more min today")
        case .addSite(let domain): return String(localized: "Allow \(domain) during focus")
        case .laterBedtime(let minutes): return String(localized: "Sleep \(Int(minutes)) min later (changes the schedule)")
        case .restDay: return String(localized: "Rest day today")
        case .configChange: return String(localized: "A change of settings")
        }
    }
}
