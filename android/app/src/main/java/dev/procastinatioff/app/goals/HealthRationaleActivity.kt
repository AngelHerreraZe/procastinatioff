package dev.procastinatioff.app.goals

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.res.stringResource
import dev.procastinatioff.app.R
import dev.procastinatioff.app.ui.Look
import dev.procastinatioff.app.ui.PcoTheme
import dev.procastinatioff.app.ui.PrimaryButton
import dev.procastinatioff.app.ui.Screen

/** Health Connect opens this from its permission screen ("privacy policy"): what is read and why. */
class HealthRationaleActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            PcoTheme(Look.CALMA) {
                Screen(
                    title = stringResource(R.string.health_rationale_title),
                    bottom = { PrimaryButton(stringResource(R.string.done)) { finish() } },
                ) {
                    Text(stringResource(R.string.health_rationale_body), style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
    }
}
