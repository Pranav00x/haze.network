@file:OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)

package com.haze.wallet.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.haze.wallet.R

// The same fog/mist palette as the web wallet (oklch converted to sRGB) -
// dominant near-black indigo-tinted fog, one warm amber accent, mint for
// "in"/success, coral for "out"/danger. See haze-wallet-web/index.html's
// :root and :root[data-theme="light"] blocks for the source of truth.

private class HazePalette(
    val fog0: Color, val fog1: Color, val fog2: Color, val fog3: Color,
    val mist: Color, val ink: Color, val amber: Color, val ok: Color, val danger: Color,
    val glow1: Color, val glow2: Color, val cardVeil: Color,
    val hairline: Color, val shadow1: Color, val shadow2: Color,
)

private val HazeDark = HazePalette(
    fog0 = Color(0xFF17161D),
    fog1 = Color(0xFF1E1D25),
    fog2 = Color(0xFF28262F),
    fog3 = Color(0xFF37343F),
    mist = Color(0xFF9C9AC7),
    ink = Color(0xFFEEEAE1),
    amber = Color(0xFFE2A75E),
    ok = Color(0xFF7ED9A3),
    danger = Color(0xFFEA7561),
    glow1 = Color(0xFF4B3E77),
    glow2 = Color(0xFF5B4326),
    cardVeil = Color(0xFF302E3A),
    hairline = Color(0x17FFFFFF),
    shadow1 = Color(0x66000000),
    shadow2 = Color(0x12FFFFFF),
)

private val HazeLight = HazePalette(
    fog0 = Color(0xFFF9F9FA),
    fog1 = Color(0xFFF2F1F4),
    fog2 = Color(0xFFE6E4E9),
    fog3 = Color(0xFFD1CFD8),
    mist = Color(0xFF6D64A8),
    ink = Color(0xFF302D38),
    amber = Color(0xFFB97A2E),
    ok = Color(0xFF2F8F5D),
    danger = Color(0xFFC2452C),
    glow1 = Color(0xFFD8D2EC),
    glow2 = Color(0xFFE9D9BE),
    cardVeil = Color(0xFFEDEBF0),
    hairline = Color(0x14000000),
    shadow1 = Color(0x1F000000),
    shadow2 = Color(0x99FFFFFF),
)

/** Colors the Material3 ColorScheme has no exact slot for - the glow gradients,
 * the frosted-glass veil, and the hairline/shadow pair the bottom nav needs. */
data class HazeExtendedColors(
    val fog2: Color,
    val fog3: Color,
    val mist: Color,
    val inkFaint: Color,
    val amberDim: Color,
    val glow1: Color,
    val glow2: Color,
    val cardVeil: Color,
    val hairline: Color,
    val shadow1: Color,
    val shadow2: Color,
    val isDark: Boolean,
)

val LocalHazeColors = compositionLocalOf {
    HazeExtendedColors(
        fog2 = HazeDark.fog2, fog3 = HazeDark.fog3, mist = HazeDark.mist,
        inkFaint = HazeDark.ink.copy(alpha = 0.34f), amberDim = HazeDark.amber.copy(alpha = 0.16f),
        glow1 = HazeDark.glow1, glow2 = HazeDark.glow2, cardVeil = HazeDark.cardVeil,
        hairline = HazeDark.hairline, shadow1 = HazeDark.shadow1, shadow2 = HazeDark.shadow2,
        isDark = true,
    )
}

private val frauncesFamily = FontFamily(
    Font(
        R.font.fraunces_variable,
        weight = FontWeight.Normal,
        variationSettings = FontVariation.Settings(FontVariation.weight(460), FontVariation.Setting("opsz", 28f)),
    ),
    Font(
        R.font.fraunces_variable,
        weight = FontWeight.SemiBold,
        variationSettings = FontVariation.Settings(FontVariation.weight(560), FontVariation.Setting("opsz", 28f)),
    ),
)

private val publicSansFamily = FontFamily(
    Font(R.font.public_sans_variable, weight = FontWeight.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(400))),
    Font(R.font.public_sans_variable, weight = FontWeight.Medium, variationSettings = FontVariation.Settings(FontVariation.weight(500))),
    Font(R.font.public_sans_variable, weight = FontWeight.SemiBold, variationSettings = FontVariation.Settings(FontVariation.weight(600))),
    Font(R.font.public_sans_variable, weight = FontWeight.Bold, variationSettings = FontVariation.Settings(FontVariation.weight(700))),
)

val plexMonoFamily = FontFamily(
    Font(R.font.ibm_plex_mono_regular, weight = FontWeight.Normal),
    Font(R.font.ibm_plex_mono_medium, weight = FontWeight.Medium),
)

private val hazeTypography = Typography(
    displaySmall = TextStyle(fontFamily = frauncesFamily, fontWeight = FontWeight.SemiBold, fontSize = 34.sp, letterSpacing = (-0.3).sp),
    headlineMedium = TextStyle(fontFamily = frauncesFamily, fontWeight = FontWeight.SemiBold, fontSize = 26.sp, letterSpacing = (-0.2).sp),
    titleLarge = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.SemiBold, fontSize = 18.sp),
    titleMedium = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.SemiBold, fontSize = 15.sp),
    titleSmall = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.Medium, fontSize = 13.sp),
    bodyLarge = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.Normal, fontSize = 15.sp, lineHeight = 22.sp),
    bodyMedium = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.Normal, fontSize = 13.5.sp, lineHeight = 20.sp),
    bodySmall = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.Normal, fontSize = 12.sp, lineHeight = 17.sp),
    labelSmall = TextStyle(fontFamily = publicSansFamily, fontWeight = FontWeight.SemiBold, fontSize = 10.5.sp, letterSpacing = 1.2.sp),
    labelMedium = TextStyle(fontFamily = plexMonoFamily, fontWeight = FontWeight.Normal, fontSize = 11.sp),
)

@Composable
fun HazeTheme(darkTheme: Boolean = isSystemInDarkTheme(), content: @Composable () -> Unit) {
    val palette = if (darkTheme) HazeDark else HazeLight

    val colorScheme = if (darkTheme) {
        darkColorScheme(
            background = palette.fog0, surface = palette.fog1, surfaceVariant = palette.fog2,
            outline = palette.fog3, primary = palette.amber, onPrimary = palette.fog0,
            primaryContainer = palette.amber.copy(alpha = 0.16f), onPrimaryContainer = palette.amber,
            secondary = palette.mist, tertiary = palette.ok, error = palette.danger,
            onBackground = palette.ink, onSurface = palette.ink,
            onSurfaceVariant = palette.ink.copy(alpha = 0.62f),
        )
    } else {
        lightColorScheme(
            background = palette.fog0, surface = palette.fog1, surfaceVariant = palette.fog2,
            outline = palette.fog3, primary = palette.amber, onPrimary = Color.White,
            primaryContainer = palette.amber.copy(alpha = 0.12f), onPrimaryContainer = palette.amber,
            secondary = palette.mist, tertiary = palette.ok, error = palette.danger,
            onBackground = palette.ink, onSurface = palette.ink,
            onSurfaceVariant = palette.ink.copy(alpha = 0.68f),
        )
    }

    val extended = HazeExtendedColors(
        fog2 = palette.fog2, fog3 = palette.fog3, mist = palette.mist,
        inkFaint = palette.ink.copy(alpha = if (darkTheme) 0.34f else 0.42f),
        amberDim = palette.amber.copy(alpha = if (darkTheme) 0.16f else 0.12f),
        glow1 = palette.glow1, glow2 = palette.glow2, cardVeil = palette.cardVeil,
        hairline = palette.hairline, shadow1 = palette.shadow1, shadow2 = palette.shadow2,
        isDark = darkTheme,
    )

    // Button/OutlinedButton/TextButton/Chip all default to shapes.small - a
    // single override here makes every button in the app a full pill,
    // matching the liquid-glass mockup, without touching each call site.
    val hazeShapes = Shapes(
        extraSmall = RoundedCornerShape(10.dp),
        small = RoundedCornerShape(50),
        medium = RoundedCornerShape(18.dp),
        large = RoundedCornerShape(22.dp),
        extraLarge = RoundedCornerShape(28.dp),
    )

    CompositionLocalProvider(LocalHazeColors provides extended) {
        MaterialTheme(colorScheme = colorScheme, typography = hazeTypography, shapes = hazeShapes, content = content)
    }
}
