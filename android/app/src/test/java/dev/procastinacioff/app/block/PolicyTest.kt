package dev.procastinacioff.app.block

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.procastinacioff_core.Config
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.Schedule
import uniffi.procastinacioff_core.TimeOfDay

class PolicyTest {
    private val wa = "android:com.whatsapp@aaaa"
    private val udemy = "android:com.udemy.android@bbbb"
    private val cfg = Config(
        schedule = Schedule(TimeOfDay(390u), TimeOfDay(1350u)),
        permitsPerDay = 3u, permitMinutes = 5u, permitCooldownMinutes = 60u,
        emergencyMinutes = 15u, emergencyPenalty = 1u, habitPenalty = true,
        goals = emptyList(), freeDays = 0u, essentialApps = listOf(wa), focusApps = listOf(udemy), webAllowlist = emptyList(),
        weeklyBonusPermits = 0u,
    )
    private val system = setOf("com.android.systemui", "com.google.android.dialer")

    @Test fun lockedAllowsEssentialOnly() {
        assertEquals(Verdict.ALLOW, Policy.decide(DayState.Locked, "com.whatsapp", wa, cfg, system))
        assertEquals(Verdict.BLOCK, Policy.decide(DayState.Locked, "com.instagram.android", "android:com.instagram.android@cccc", cfg, system))
    }

    @Test fun fakeWhatsappWithOtherCertIsBlocked() {
        assertEquals(Verdict.BLOCK, Policy.decide(DayState.Locked, "com.whatsapp", "android:com.whatsapp@ffff", cfg, system))
    }

    @Test fun focusAddsFocusApps() {
        assertEquals(Verdict.ALLOW, Policy.decide(DayState.Focus("leer-30"), "com.udemy.android", udemy, cfg, system))
    }

    @Test fun sleepOnlySystem() {
        assertEquals(Verdict.BLOCK, Policy.decide(DayState.Sleep, "com.whatsapp", wa, cfg, system))
        assertEquals(Verdict.ALLOW, Policy.decide(DayState.Sleep, "com.google.android.dialer", null, cfg, system))
    }

    @Test fun freeDayAllowsEverything() {
        // The core returns FREE on free days: nothing is blocked
        assertEquals(Verdict.ALLOW, Policy.decide(DayState.Free, "com.instagram.android", null, cfg, system))
    }
}
