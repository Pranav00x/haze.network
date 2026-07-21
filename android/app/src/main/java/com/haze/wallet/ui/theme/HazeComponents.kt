package com.haze.wallet.ui.theme

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp

/** The frosted-glass card treatment the wallet home screen's balance card
 * already used - pulled out so every screen gets the same veil+hairline
 * look instead of falling back to Material3's plain default Card. Carries
 * a faint diagonal specular highlight along the top edge (the same "light
 * catching the glass" cue the liquid-glass mockup uses), not just a flat
 * fill + border. */
@Composable
fun HazeCard(
    modifier: Modifier = Modifier,
    padding: PaddingValues = PaddingValues(16.dp),
    content: @Composable () -> Unit,
) {
    val colors = LocalHazeColors.current
    val shape = RoundedCornerShape(18.dp)
    Surface(
        shape = shape,
        color = colors.cardVeil.copy(alpha = if (colors.isDark) 0.5f else 0.7f),
        border = BorderStroke(1.dp, colors.hairline),
        modifier = modifier,
    ) {
        Box(
            modifier = Modifier
                .background(
                    Brush.linearGradient(
                        colors = listOf(Color.White.copy(alpha = if (colors.isDark) 0.06f else 0.35f), Color.Transparent),
                        start = androidx.compose.ui.geometry.Offset(0f, 0f),
                        end = androidx.compose.ui.geometry.Offset(240f, 200f),
                    ),
                    shape = shape,
                )
                .padding(padding),
        ) { content() }
    }
}

/** A screen's main title - Fraunces (the same display serif the web
 * wallet's headings use), not the body sans used for subsection labels. */
@Composable
fun HazeScreenTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, style = MaterialTheme.typography.headlineMedium, modifier = modifier)
}

/** Several large, softly blurred color blobs behind the whole app - the
 * "liquid" in liquid glass, what every glass panel's translucency is
 * actually catching. Mirrors the web mockup's .ambient blobs. Meant to
 * sit at the root of the screen, behind all content. */
@Composable
fun HazeAmbientBlobs(modifier: Modifier = Modifier) {
    val colors = LocalHazeColors.current
    Box(modifier = modifier.fillMaxSize()) {
        Box(
            Modifier
                .size(260.dp)
                .offset(x = (-70).dp, y = (-60).dp)
                .background(colors.mist.copy(alpha = 0.22f), CircleShape)
                .blur(90.dp),
        )
        Box(
            Modifier
                .size(220.dp)
                .align(Alignment.BottomEnd)
                .offset(x = 60.dp, y = 40.dp)
                .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.20f), CircleShape)
                .blur(90.dp),
        )
        Box(
            Modifier
                .size(180.dp)
                .align(Alignment.Center)
                .offset(x = 90.dp, y = (-120).dp)
                .background(colors.hazeOk().copy(alpha = 0.10f), CircleShape)
                .blur(90.dp),
        )
    }
}

private fun HazeExtendedColors.hazeOk(): Color = if (isDark) Color(0xFF7ED9A3) else Color(0xFF2F8F5D)

/** A circular icon action with a label underneath - Send/Receive/Faucet
 * on the wallet home screen. `primary = true` gives it the same solid-
 * amber emphasis the mockup's Send button gets; the rest sit as a dim
 * fog-tinted circle so the primary action doesn't have to compete. */
@Composable
fun HazeQuickAction(
    label: String,
    icon: ImageVector,
    primary: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val colors = LocalHazeColors.current
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Surface(
            shape = CircleShape,
            color = (if (primary) MaterialTheme.colorScheme.primary else colors.fog2).copy(alpha = if (enabled) 1f else 0.4f),
            modifier = Modifier.size(52.dp),
            enabled = enabled,
            onClick = onClick,
        ) {
            Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize()) {
                Icon(
                    icon,
                    contentDescription = label,
                    tint = if (primary) MaterialTheme.colorScheme.background else MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(20.dp),
                )
            }
        }
        Text(label, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(top = 6.dp))
    }
}

/** A small dot with an outward-pulsing ring - "this is live" (node status,
 * online indicators). Mirrors the mockup's .node-pulse animation. */
@Composable
fun HazePulseDot(color: Color = LocalHazeColors.current.let { if (it.isDark) Color(0xFF7ED9A3) else Color(0xFF2F8F5D) }, modifier: Modifier = Modifier) {
    val transition = rememberInfiniteTransition(label = "pulse")
    val scale by transition.animateFloat(
        initialValue = 1f, targetValue = 2.6f,
        animationSpec = infiniteRepeatable(tween(1600), RepeatMode.Restart),
        label = "pulseScale",
    )
    val ringAlpha by transition.animateFloat(
        initialValue = 0.45f, targetValue = 0f,
        animationSpec = infiniteRepeatable(tween(1600), RepeatMode.Restart),
        label = "pulseAlpha",
    )
    Box(modifier = modifier.size(14.dp), contentAlignment = Alignment.Center) {
        Box(
            Modifier
                .size((6 * scale).dp)
                .background(color.copy(alpha = ringAlpha), CircleShape),
        )
        Box(Modifier.size(6.dp).background(color, CircleShape))
    }
}
