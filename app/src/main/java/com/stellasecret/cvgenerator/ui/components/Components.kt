package com.stellasecret.cvgenerator.ui.components

import androidx.compose.animation.*
import androidx.compose.animation.core.*
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.stellasecret.cvgenerator.ui.theme.*

// ── Gradient Header ────────────────────────────────────────────────────────────

@Composable
fun GradientHeader(modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(200.dp)
            .background(
                Brush.verticalGradient(
                    colors = listOf(Navy600, Navy900)
                )
            )
            .drawBehind {
                // Decorative circles
                drawCircle(ElectricBlue.copy(alpha = 0.08f), radius = 300f, center = Offset(size.width * 0.85f, 80f))
                drawCircle(Gold.copy(alpha = 0.05f), radius = 200f, center = Offset(60f, size.height * 0.8f))
            },
        contentAlignment = Alignment.Center
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Box(
                    modifier = Modifier
                        .size(48.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .background(ElectricBlue),
                    contentAlignment = Alignment.Center
                ) {
                    Text("CV", color = Color.White, fontWeight = FontWeight.Black, fontSize = 16.sp)
                }
                Text(
                    text = "CVGenerator",
                    style = MaterialTheme.typography.headlineMedium,
                    color = Color.White,
                    fontWeight = FontWeight.Black
                )
            }
            Spacer(Modifier.height(8.dp))
            Text(
                text = "Votre CV optimisé par l'IA",
                style = MaterialTheme.typography.bodyMedium,
                color = Slate400
            )
        }
    }
}

// ── Upload Card ────────────────────────────────────────────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun UploadCard(
    title: String,
    subtitle: String,
    icon: ImageVector,
    isLoaded: Boolean,
    loadedFileName: String? = null,
    isLoading: Boolean = false,
    error: String? = null,
    onClick: () -> Unit,
    onClear: (() -> Unit)? = null,
    modifier: Modifier = Modifier
) {
    val borderColor = when {
        error != null -> MaterialTheme.colorScheme.error
        isLoaded -> Emerald
        else -> MaterialTheme.colorScheme.outline
    }

    Card(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant
        ),
        border = BorderStroke(
            width = if (isLoaded) 2.dp else 1.dp,
            color = borderColor
        ),
        onClick = if (!isLoaded) onClick else ({})
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            // Icon box
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(
                        if (isLoaded) Emerald.copy(alpha = 0.15f)
                        else MaterialTheme.colorScheme.primary.copy(alpha = 0.1f)
                    ),
                contentAlignment = Alignment.Center
            ) {
                if (isLoading) {
                    CircularProgressIndicator(modifier = Modifier.size(24.dp), strokeWidth = 2.dp)
                } else {
                    Icon(
                        imageVector = if (isLoaded) Icons.Filled.CheckCircle else icon,
                        contentDescription = null,
                        tint = if (isLoaded) Emerald else MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(24.dp)
                    )
                }
            }

            // Text content
            Column(modifier = Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(2.dp))
                if (isLoaded && loadedFileName != null) {
                    Text(
                        text = loadedFileName,
                        style = MaterialTheme.typography.bodySmall,
                        color = Emerald,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis
                    )
                } else if (error != null) {
                    Text(
                        text = error,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error
                    )
                } else {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }

            // Actions
            if (isLoaded && onClear != null) {
                IconButton(onClick = onClear) {
                    Icon(Icons.Filled.Close, "Supprimer", tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else if (!isLoaded) {
                Icon(
                    Icons.Filled.ChevronRight, null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
        }
    }
}

// ── Section Header ─────────────────────────────────────────────────────────────

@Composable
fun SectionHeader(title: String, badge: String? = null) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onBackground
        )
        badge?.let {
            Surface(
                shape = RoundedCornerShape(50),
                color = ElectricBlue.copy(alpha = 0.15f)
            ) {
                Text(
                    text = it,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                    style = MaterialTheme.typography.labelSmall,
                    color = ElectricBlue
                )
            }
        }
    }
}

// ── Premium Badge ──────────────────────────────────────────────────────────────

@Composable
fun PremiumBadge() {
    Surface(
        shape = RoundedCornerShape(50),
        color = Gold.copy(alpha = 0.15f)
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(Icons.Filled.Star, null, tint = Gold, modifier = Modifier.size(14.dp))
            Text("Premium", style = MaterialTheme.typography.labelSmall, color = Gold, fontWeight = FontWeight.Bold)
        }
    }
}

// ── Animated Generate Button ───────────────────────────────────────────────────

@Composable
fun GenerateButton(
    enabled: Boolean,
    isLoading: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    Button(
        onClick = onClick,
        enabled = enabled && !isLoading,
        modifier = modifier
            .fillMaxWidth()
            .height(56.dp),
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = ElectricBlue,
            disabledContainerColor = MaterialTheme.colorScheme.outline
        )
    ) {
        if (isLoading) {
            CircularProgressIndicator(
                modifier = Modifier.size(24.dp),
                color = Color.White,
                strokeWidth = 2.dp
            )
            Spacer(Modifier.width(12.dp))
            Text("Génération en cours...", color = Color.White, fontWeight = FontWeight.SemiBold)
        } else {
            Icon(Icons.Filled.AutoAwesome, null, tint = Color.White)
            Spacer(Modifier.width(8.dp))
            Text("Générer mon CV", color = Color.White, fontWeight = FontWeight.Bold, fontSize = 16.sp)
        }
    }
}

// ── Info chip ─────────────────────────────────────────────────────────────────

@Composable
fun InfoChip(text: String, icon: ImageVector, color: Color = ElectricBlue) {
    Surface(
        shape = RoundedCornerShape(50),
        color = color.copy(alpha = 0.1f)
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(icon, null, tint = color, modifier = Modifier.size(14.dp))
            Text(text, style = MaterialTheme.typography.labelSmall, color = color)
        }
    }
}
