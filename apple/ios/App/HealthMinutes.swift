// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import HealthKit

/// Workout minutes from Apple Health, for "Workout" goals (`Check.health`). Only reads, only today's total of
/// Apple's exercise minutes (watch or iPhone), never single samples, routes or heart rate. The result goes to
/// `Core.completeGoal(method: .health, minutes:)`, which needs at least the goal's minutes.
/// Same role as Android's HealthMinutes.kt. NOT COMPILED HERE (no Mac): VERIFY against the iOS SDK.
enum HealthMinutes {
    private static let store = HKHealthStore()
    private static let exercise = HKQuantityType(.appleExerciseTime)

    static var available: Bool { HKHealthStore.isHealthDataAvailable() }

    /// Asks once. HealthKit never says whether reading was allowed: a denial just reads 0.
    static func authorize() async throws {
        try await store.requestAuthorization(toShare: [], read: [exercise])
    }

    /// Exercise minutes since `fromMs` (the day's start, from `Status.dayStartMs`).
    static func since(fromMs: Int64) async throws -> Int {
        let start = Date(timeIntervalSince1970: Double(fromMs) / 1000)
        let predicate = HKQuery.predicateForSamples(withStart: start, end: Date())
        let query = HKStatisticsQueryDescriptor(
            predicate: HKSamplePredicate.quantitySample(type: exercise, predicate: predicate),
            options: .cumulativeSum)
        let total = try await query.result(for: store)?.sumQuantity()?.doubleValue(for: .minute()) ?? 0
        return Int(total)
    }
}
