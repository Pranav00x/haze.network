package com.haze.wallet.ui.theme

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/** The frosted-glass card treatment the wallet home screen's balance card
 * already used - pulled out so every screen gets the same veil+hairline
 * look instead of falling back to Material3's plain default Card. */
@Composable
fun HazeCard(
    modifier: Modifier = Modifier,
    padding: PaddingValues = PaddingValues(16.dp),
    content: @Composable () -> Unit,
) {
    val colors = LocalHazeColors.current
    Surface(
        shape = RoundedCornerShape(18.dp),
        color = colors.cardVeil.copy(alpha = if (colors.isDark) 0.5f else 0.7f),
        border = BorderStroke(1.dp, colors.hairline),
        modifier = modifier,
    ) {
        androidx.compose.foundation.layout.Box(modifier = Modifier.padding(padding)) { content() }
    }
}

/** A screen's main title - Fraunces (the same display serif the web
 * wallet's headings use), not the body sans used for subsection labels. */
@Composable
fun HazeScreenTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, style = MaterialTheme.typography.headlineMedium, modifier = modifier)
}
