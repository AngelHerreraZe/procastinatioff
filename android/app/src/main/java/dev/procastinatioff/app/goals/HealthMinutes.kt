package dev.procastinatioff.app.goals

import android.content.Context
import androidx.health.connect.client.HealthConnectClient
import androidx.health.connect.client.permission.HealthPermission
import androidx.health.connect.client.records.ExerciseSessionRecord
import androidx.health.connect.client.request.AggregateRequest
import androidx.health.connect.client.time.TimeRangeFilter
import java.time.Instant

/**
 * Exercise minutes from Health Connect (Android 14+ built in; Android 9-13 with its app). Only reads; only
 * today's total, never single records, routes or heart rate. Not on phones without Health Connect (e.g.
 * Huawei without Google services): there the goal keeps the declaration.
 */
object HealthMinutes {
    val PERMISSIONS = setOf(HealthPermission.getReadPermission(ExerciseSessionRecord::class))

    fun available(ctx: Context): Boolean = HealthConnectClient.getSdkStatus(ctx) == HealthConnectClient.SDK_AVAILABLE

    suspend fun granted(ctx: Context): Boolean =
        HealthConnectClient.getOrCreate(ctx).permissionController.getGrantedPermissions().containsAll(PERMISSIONS)

    /** Minutes of exercise sessions since `fromMs` (the day's start). Health Connect merges overlapping sources. */
    suspend fun since(ctx: Context, fromMs: Long): Int {
        val result = HealthConnectClient.getOrCreate(ctx).aggregate(
            AggregateRequest(
                metrics = setOf(ExerciseSessionRecord.EXERCISE_DURATION_TOTAL),
                timeRangeFilter = TimeRangeFilter.between(Instant.ofEpochMilli(fromMs), Instant.now()),
            ),
        )
        return result[ExerciseSessionRecord.EXERCISE_DURATION_TOTAL]?.toMinutes()?.toInt() ?: 0
    }
}
