package com.stellasecret.cvgenerator.ui.theme

import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

// ── Brand palette – deep navy + electric blue + warm gold ─────────────────────
val Navy900    = Color(0xFF0A0F1E)
val Navy800    = Color(0xFF111827)
val Navy700    = Color(0xFF1A2539)
val Navy600    = Color(0xFF1E3A5F)
val ElectricBlue = Color(0xFF3B82F6)
val CyanAccent   = Color(0xFF06B6D4)
val Gold         = Color(0xFFF59E0B)
val GoldLight    = Color(0xFFFBBF24)
val Emerald      = Color(0xFF10B981)
val Rose         = Color(0xFFF43F5E)
val White        = Color(0xFFFFFFFF)
val Slate100     = Color(0xFFF1F5F9)
val Slate200     = Color(0xFFE2E8F0)
val Slate400     = Color(0xFF94A3B8)
val Slate600     = Color(0xFF475569)

val DarkColorScheme = darkColorScheme(
    primary       = ElectricBlue,
    onPrimary     = White,
    primaryContainer   = Navy600,
    onPrimaryContainer = Slate100,
    secondary     = Gold,
    onSecondary   = Navy900,
    secondaryContainer = Color(0xFF78350F),
    onSecondaryContainer = GoldLight,
    tertiary      = Emerald,
    background    = Navy900,
    onBackground  = Slate100,
    surface       = Navy800,
    onSurface     = Slate100,
    surfaceVariant = Navy700,
    onSurfaceVariant = Slate400,
    error         = Rose,
    onError       = White,
    outline       = Slate600,
)

val LightColorScheme = lightColorScheme(
    primary       = Color(0xFF1D4ED8),
    onPrimary     = White,
    primaryContainer   = Color(0xFFDBEAFE),
    onPrimaryContainer = Color(0xFF1E3A5F),
    secondary     = Color(0xFFD97706),
    onSecondary   = White,
    secondaryContainer = Color(0xFFFEF3C7),
    onSecondaryContainer = Color(0xFF78350F),
    tertiary      = Color(0xFF059669),
    background    = Slate100,
    onBackground  = Navy900,
    surface       = White,
    onSurface     = Navy800,
    surfaceVariant = Color(0xFFE2E8F0),
    onSurfaceVariant = Slate600,
    error         = Color(0xFFDC2626),
    onError       = White,
    outline       = Slate400,
)
