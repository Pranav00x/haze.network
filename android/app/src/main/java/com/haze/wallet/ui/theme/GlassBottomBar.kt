package com.haze.wallet.ui.theme

import android.graphics.Shader
import android.os.Build
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.ripple.rememberRipple
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asComposeRenderEffect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import android.graphics.RenderEffect as AndroidRenderEffect

/** One entry in the floating bottom bar - either a navigable screen (`route`
 * set) or a one-off action like opening the block explorer (`route` null,
 * `onAction` set instead). */
data class HazeNavItem(
    val key: String,
    val label: String,
    val icon: ImageVector,
    val route: String? = null,
    val onAction: (() -> Unit)? = null,
)

/** True backdrop blur on API 31+ (the actual glass in "glassmorphism");
 * a flat frosted veil everywhere else, since RenderEffect blur has no
 * pre-31 equivalent worth faking. */
private fun Modifier.frostedGlass(radius: Float = 28f): Modifier = this.graphicsLayer {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        renderEffect = AndroidRenderEffect
            .createBlurEffect(radius, radius, Shader.TileMode.CLAMP)
            .asComposeRenderEffect()
    }
}

@Composable
fun HazeGlassBottomBar(
    items: List<HazeNavItem>,
    currentRoute: String?,
    onNavigate: (String) -> Unit,
) {
    val colors = LocalHazeColors.current

    Box(
        modifier = Modifier.fillMaxWidth().navigationBarsPadding(),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Box(
            modifier = Modifier
                .padding(bottom = 14.dp)
                .clip(RoundedCornerShape(50))
                .frostedGlass()
                .background(colors.cardVeil.copy(alpha = if (colors.isDark) 0.72f else 0.82f))
                .border(1.dp, colors.hairline, RoundedCornerShape(50))
        ) {
            Row(
                modifier = Modifier
                    .horizontalScroll(rememberScrollState())
                    .padding(6.dp),
                horizontalArrangement = Arrangement.spacedBy(2.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                items.forEach { item ->
                    val selected = item.route != null && item.route == currentRoute
                    HazeNavCircle(
                        item = item,
                        selected = selected,
                        onClick = { item.onAction?.invoke() ?: item.route?.let(onNavigate) },
                    )
                }
            }
        }
    }
}

@Composable
private fun HazeNavCircle(item: HazeNavItem, selected: Boolean, onClick: () -> Unit) {
    val colors = LocalHazeColors.current
    val amber = MaterialTheme.colorScheme.primary
    val size by animateDpAsState(if (selected) 50.dp else 44.dp, tween(220), label = "navCircleSize")
    val bg by animateColorAsState(
        if (selected) colors.amberDim else Color.Transparent,
        tween(220), label = "navCircleBg",
    )
    val tint by animateColorAsState(if (selected) amber else colors.inkFaint, tween(220), label = "navCircleTint")

    Box(
        modifier = Modifier
            .size(size)
            .clip(CircleShape)
            .background(bg)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = rememberRipple(bounded = true, radius = 26.dp),
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Icon(item.icon, contentDescription = item.label, tint = tint, modifier = Modifier.size(20.dp))
    }
}
