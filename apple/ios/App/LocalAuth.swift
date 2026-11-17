// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import LocalAuthentication
import Security

/// Biometrics as a local method (ADR 0013), the iOS counterpart of LocalAuth.kt. SECURITY:
/// - Success is tied to the Secure Enclave releasing a Keychain item protected with
///   `.biometryCurrentSet`, not only to an `evaluatePolicy` callback.
/// - `.biometryCurrentSet`: if someone enrolls another finger or face (e.g. a minor on the guardian's
///   iPhone), the item is invalidated and biometrics must be turned on again with the PIN.
/// - `WhenPasscodeSetThisDeviceOnly`: never in backups nor on another device.
/// - No device-passcode fallback: the fallback is the app's own PIN, password or pattern.
///
/// Turning it on (settings): `enable()`, then `confirm`, then add `.biometric` to the policy with
/// `setConfig` (tightening), as Android does.
enum LocalAuth {
    private static var item: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: "dev.procastinacioff.biometric",
         kSecAttrAccount as String: "gate-v1"]
    }

    static var available: Bool {
        LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil)
    }

    /// Creates (or replaces) the protected item.
    static func enable() throws {
        guard let access = SecAccessControlCreateWithFlags(
            nil, kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly, .biometryCurrentSet, nil)
        else { throw CoreError.Secret(msg: "access control") }
        var random = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, random.count, &random) == errSecSuccess else {
            throw CoreError.Secret(msg: "random")
        }
        SecItemDelete(item as CFDictionary)
        var add = item
        add[kSecValueData as String] = Data(random)
        add[kSecAttrAccessControl as String] = access
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw CoreError.Secret(msg: "keychain \(status)") }
    }

    static func disable() {
        SecItemDelete(item as CFDictionary)
    }

    /// true only if the OS confirmed biometrics AND released the item. false: cancelled, failed,
    /// never enabled, or invalidated by a new enrollment (offer the PIN). Blocks while the system
    /// prompt is shown: call off the main thread.
    static func confirm(reason: String) -> Bool {
        let context = LAContext()
        context.localizedReason = reason
        context.localizedFallbackTitle = ""
        var query = item
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context
        var out: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess else { return false }
        return (out as? Data)?.count == 32
    }

    /// Opens the core's local gate for 5 minutes with biometrics. false: not confirmed.
    static func authorize(_ core: Core, reason: String) async throws -> Bool {
        guard await Task.detached(operation: { confirm(reason: reason) }).value else { return false }
        try core.authorizeBiometric(clock: CoreHolder.clock())
        return true
    }
}
