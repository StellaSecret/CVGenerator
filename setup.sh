#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# setup.sh — À exécuter UNE FOIS après le clone, avant le premier build CI.
#
# Ce script génère le gradle-wrapper.jar manquant via une installation Gradle
# locale, puis le commite dans le dépôt.
#
# Prérequis : Gradle installé localement (brew install gradle  /  sdk install gradle)
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

GRADLE_VERSION="8.4"

echo "▶ Vérification de Gradle local..."
if ! command -v gradle &>/dev/null; then
  echo "❌ Gradle non trouvé. Installez-le :"
  echo "   macOS  : brew install gradle"
  echo "   Linux  : sdk install gradle ${GRADLE_VERSION}  (via SDKMAN)"
  echo "   Windows: scoop install gradle"
  exit 1
fi

echo "▶ Génération du Gradle Wrapper (v${GRADLE_VERSION})..."
gradle wrapper --gradle-version "${GRADLE_VERSION}" --distribution-type bin

echo "▶ Ajout des droits d'exécution..."
chmod +x gradlew

echo "▶ Vérification du jar généré..."
JAR_PATH="gradle/wrapper/gradle-wrapper.jar"
if [ ! -f "$JAR_PATH" ]; then
  echo "❌ Le jar n'a pas été généré correctement."
  exit 1
fi
echo "   ✅ $JAR_PATH — $(du -h "$JAR_PATH" | cut -f1)"

echo ""
echo "✅ Setup terminé. Vous pouvez maintenant commiter :"
echo "   git add gradlew gradlew.bat gradle/wrapper/"
echo "   git commit -m 'chore: add gradle wrapper'"
echo "   git push"
