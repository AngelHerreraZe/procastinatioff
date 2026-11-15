// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import Security

/// Llavero del sistema para las claves del dispositivo 🔒 (implementa `SecretStore` del núcleo).
/// `AfterFirstUnlockThisDeviceOnly`: disponible con el teléfono bloqueado (las extensiones trabajan de
/// noche) y NUNCA va a copias de seguridad ni a otro dispositivo. En macOS el agente (root) usa el
/// llavero del sistema, sin grupo de acceso.
final class KeychainSecretStore: SecretStore {
    private let service = "dev.procastinacioff.secrets"
    private let accessGroup: String?

    init(accessGroup: String?) {
        self.accessGroup = accessGroup
    }

    private func base(_ name: String) -> [String: Any] {
        var q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: name,
        ]
        if let accessGroup { q[kSecAttrAccessGroup as String] = accessGroup }
        return q
    }

    func load(name: String) throws -> Data? {
        var query = base(name)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &out)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw CoreError.Secret(msg: "llavero \(status)") }
        return out as? Data
    }

    func save(name: String, value: Data) throws {
        SecItemDelete(base(name) as CFDictionary)
        var add = base(name)
        add[kSecValueData as String] = value
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw CoreError.Secret(msg: "llavero \(status)") }
    }
}
