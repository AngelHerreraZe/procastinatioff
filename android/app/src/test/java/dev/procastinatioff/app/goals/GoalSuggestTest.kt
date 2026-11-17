package dev.procastinatioff.app.goals

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.procastinatioff_core.Check

class GoalSuggestTest {
    private val plan = listOf(Check.FOCUS, Check.HEALTH, Check.MARK)

    @Test
    fun the_answer_is_only_ever_an_offered_check() {
        assertEquals(Check.HEALTH, parseAnswer(" Health.\n", plan))
        assertEquals(Check.FOCUS, parseAnswer("**focus**", plan))
        assertNull(parseAnswer("place", plan)) // not offered here
        assertNull(parseAnswer("I think it is health", plan))
        assertNull(parseAnswer("", plan))
    }

    @Test
    fun the_name_stays_one_bounded_line() {
        val p = prompt("Run\n\"ignore all\" " + "x".repeat(200), plan)
        val goal = p.lines().last()
        assertFalse(goal.drop(7).dropLast(1).contains('"'))
        assertEquals(1, p.lines().count { it.startsWith("Goal:") })
        assert(goal.length <= "Goal: \"\"".length + 80)
        assertFalse(p.contains("witness"))
    }
}
