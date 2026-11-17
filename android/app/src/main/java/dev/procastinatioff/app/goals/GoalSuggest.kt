package dev.procastinatioff.app.goals

import com.google.mlkit.genai.common.FeatureStatus
import com.google.mlkit.genai.prompt.Generation
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.procastinatioff_core.Check
import uniffi.procastinatioff_core.suggestChecks

/** How to check a goal, suggested from its name. `byAi`: it came from this phone's own model. */
data class Suggestion(val check: Check, val byAi: Boolean)

/**
 * First the phone's own model (Gemini Nano through AICore, with the ML Kit GenAI Prompt API: Pixel, Galaxy and
 * other recent phones) when it is already downloaded; otherwise the core's keyword catalog, which works on every
 * phone (Huawei and others without AICore). Only a suggestion: the person picks, and the core only accepts the
 * methods of the check they pick.
 * SECURITY: The goal's name never leaves the phone: AICore runs the model on the device. The answer is reduced
 * to one of `offered`, so a name that tries to steer the model can at most pick another offered check.
 */
object GoalSuggest {
    /** ponytail: a slow model just falls back to the catalog; raise it if suggestions arrive late on real phones. */
    private const val AI_TIMEOUT_MS = 2_500L

    suspend fun suggest(title: String, offered: List<Check>): Suggestion? {
        if (title.isBlank()) return null
        onDevice(title, offered)?.let { return Suggestion(it, byAi = true) }
        return suggestChecks(title).firstOrNull { it in offered }?.let { Suggestion(it, byAi = false) }
    }

    private suspend fun onDevice(title: String, offered: List<Check>): Check? = withContext(Dispatchers.IO) {
        runCatching {
            withTimeoutOrNull(AI_TIMEOUT_MS) {
                val model = Generation.getClient()
                try {
                    // Never starts a download: if Gemini Nano is not ready, the catalog answers
                    if (model.checkStatus() != FeatureStatus.AVAILABLE) null
                    else parseAnswer(model.generateContent(prompt(title, offered)).candidates.firstOrNull()?.text.orEmpty(), offered)
                } finally {
                    model.close()
                }
            }
        }.getOrNull()
    }
}

private val HINTS = mapOf(
    Check.FOCUS to "timed work on the phone or computer (study, write, practice, learn a language)",
    Check.READING to "reading a book for some minutes",
    Check.PAGES to "reading some pages and writing a summary",
    Check.EXERCISE to "counting reps such as squats or push-ups",
    Check.FOOD to "eating or cooking something healthy (photo of the plate)",
    Check.PLACE to "going somewhere (gym, library, office, school) and scanning a QR there",
    Check.HEALTH to "running, walking, cycling, swimming or any workout a watch or phone records",
    Check.WITNESS to "a parent or guardian confirms it",
    Check.MARK to "anything else (chores, calls, small habits)",
)

internal fun prompt(title: String, offered: List<Check>): String = buildString {
    appendLine("Pick how to check this personal goal. Answer with one word from this list, nothing else:")
    offered.forEach { appendLine("${it.name.lowercase()}: ${HINTS[it]}") }
    // The name as data: one line, no quotes, bounded
    append("Goal: \"").append(title.replace(Regex("[\"\\r\\n]+"), " ").trim().take(80)).append('"')
}

/** The model's answer as one of `offered`, or null. */
internal fun parseAnswer(answer: String, offered: List<Check>): Check? {
    val word = answer.trim().trimStart('*', '"', '\'', '`').lowercase().takeWhile { it.isLetter() }
    return offered.firstOrNull { it.name.lowercase() == word }
}
