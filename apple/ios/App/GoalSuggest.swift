// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

/// How to check a goal, suggested from its name. First Apple's on-device model (Apple Intelligence: iPhone 15 Pro
/// and later, iOS 26) with guided generation, so the answer can only be one of the offered checks; otherwise the
/// core's keyword catalog, which works on every iPhone. Only a suggestion: the person picks, and the core only
/// accepts the methods of the check they pick.
/// SECURITY: The goal's name never leaves the iPhone (the model runs on the device).
/// NOT COMPILED HERE (no Mac): VERIFY against the iOS 26 SDK.
enum GoalSuggest {
    struct Suggestion {
        let check: Check
        let byAi: Bool
    }

    static func suggest(title: String, offered: [Check]) async -> Suggestion? {
        let name = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        if let c = await onDevice(name, offered), offered.contains(c) { return Suggestion(check: c, byAi: true) }
        return suggestChecks(title: name).first(where: offered.contains).map { Suggestion(check: $0, byAi: false) }
    }

    private static func onDevice(_ name: String, _ offered: [Check]) async -> Check? {
        #if canImport(FoundationModels)
        if #available(iOS 26.0, *) {
            guard case .available = SystemLanguageModel.default.availability else { return nil }
            let session = LanguageModelSession(instructions: """
                Pick how to check a personal goal. focus: timed work on the phone or computer (study, write, \
                practice, learn a language). reading: reading a book for some minutes. pages: reading pages and \
                writing a summary. exercise: counting reps such as squats. food: eating or cooking something healthy. \
                place: going somewhere (gym, library, office) and scanning a QR there. health: running, walking, \
                cycling or any workout a watch records. witness: a parent confirms it. mark: anything else.
                """)
            // The name as data: one line, bounded
            let goal = String(name.replacingOccurrences(of: "\n", with: " ").prefix(80))
            guard let r = try? await session.respond(to: "Goal: \(goal)", generating: CheckChoice.self) else { return nil }
            return r.content.check
        }
        #endif
        return nil
    }
}

#if canImport(FoundationModels)
/// Guided generation: the model can only answer one of these cases.
@available(iOS 26.0, *)
@Generable
enum CheckChoice {
    case focus, reading, pages, exercise, food, place, health, witness, mark

    var check: Check {
        switch self {
        case .focus: .focus
        case .reading: .reading
        case .pages: .pages
        case .exercise: .exercise
        case .food: .food
        case .place: .place
        case .health: .health
        case .witness: .witness
        case .mark: .mark
        }
    }
}
#endif
