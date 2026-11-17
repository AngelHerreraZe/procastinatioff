// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import Security

/// System keychain for the device keys; implements the core's `SecretStore` (security-critical).
/// `AfterFirstUnlockThisDeviceOnly`: available with the phone locked (the extensions work at
/// night) and NEVER goes to backups or to another device. On macOS the agent (root) uses the
/// system keychain, without an access group.
final class KeychainSecretStore: SecretStore {
    private let service = "dev.procastinatioff.secrets"
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
        guard status == errSecSuccess else { throw CoreError.Secret(msg: "keychain \(status)") }
        return out as? Data
    }

    func save(name: String, value: Data) throws {
        SecItemDelete(base(name) as CFDictionary)
        var add = base(name)
        add[kSecValueData as String] = value
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw CoreError.Secret(msg: "keychain \(status)") }
    }
}
