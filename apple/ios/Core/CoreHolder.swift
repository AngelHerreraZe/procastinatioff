// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import Security

/// Acceso único a los núcleos Rust (app y extensión de avisos). Todos comparten las claves del iPhone:
/// - core.db: el perfil de este iPhone (propio, o el del menor si es su iPhone).
/// - partner.db / watching.db: buzones de compañero (Fase 2).
/// - family-<id>.db: (tutor) un perfil por cada menor que administro. Nunca bloquean ESTE iPhone.
/// Las bases viven en el App Group con la protección por defecto (`completeUntilFirstUserAuthentication`):
/// la extensión de avisos las abre de noche con el iPhone bloqueado.
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
            throw CoreError.Storage(msg: "sin App Group")
        }
        let c = try Core.open(dbPath: dir.appendingPathComponent(file).path, secrets: KeychainSecretStore(accessGroup: keychainGroup))
        cores[file] = c
        return c
    }

    static func shared() throws -> Core { try open("core.db") }
    static func partnerBox() throws -> Core { try open("partner.db") }
    static func watchingBox() throws -> Core { try open("watching.db") }

    /// El buzón solo cuenta si ya se creó o se unió (no se abre una base vacía por curiosidad).
    static func existing(_ file: String) -> Core? {
        guard let dir = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppGroup.id),
              FileManager.default.fileExists(atPath: dir.appendingPathComponent(file).path),
              let c = try? open(file), !c.members().isEmpty else { return nil }
        return c
    }

    static func family(_ id: String) throws -> Core {
        guard id.range(of: "^[0-9a-f]{16}$", options: .regularExpression) != nil else {
            throw CoreError.Protocol(msg: "id de perfil inválido")
        }
        return try open("family-\(id).db")
    }

    static func clock() -> Clock { CoreClock.now(try? shared()) }
}

/// (Tutor) Perfiles de menores: id → nombre. Solo en este iPhone; el nombre nunca sale de aquí.
enum FamilyProfiles {
    private static let key = "family-profiles"
    private static var defaults: UserDefaults? { UserDefaults(suiteName: AppGroup.id) }

    static func all() -> [String: String] { defaults?.dictionary(forKey: key) as? [String: String] ?? [:] }

    static func newId() -> String {
        var bytes = [UInt8](repeating: 0, count: 8)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// Se registra solo cuando el perfil ya existe en el núcleo.
    static func save(id: String, name: String) {
        var all = all()
        all[id] = String(name.prefix(64))
        defaults?.set(all, forKey: key)
    }
}

enum Texts {
    static func describe(_ what: RequestKind) -> String {
        switch what {
        case .extraPermit(let minutes): return "\(minutes) min más hoy"
        case .addSite(let domain): return "Permitir \(domain) en el enfoque"
        case .laterBedtime(let minutes): return "Dormir \(minutes) min más tarde (cambia el horario)"
        case .restDay: return "Día de descanso hoy"
        }
    }
}
