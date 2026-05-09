# CVGenerator — Application Android de génération de CV par IA

CVGenerator génère des CV professionnels et adaptés aux fiches de poste à partir d'un profil LinkedIn (PDF), grâce à l'API Anthropic Claude.

---

## Fonctionnalités

| Fonctionnalité | Description |
|---|---|
| 📄 Import LinkedIn PDF | Extraction automatique du texte depuis le PDF exporté LinkedIn |
| 📋 Fiche de poste | Import fichier (PDF, DOCX, TXT…) **ou** copier-coller direct |
| 🤖 IA Anthropic | Génération via Claude Opus — CV adapté, mots-clés ATS intégrés |
| 🔐 Auth Google (OAuth) | Connexion Firebase Google Sign-In |
| ⭐ Premium (Vertex AI) | Utilisateurs premium → Vertex AI sans clé API |
| 🔑 Clé API manuelle | Utilisateurs non-premium ou non connectés → clé Anthropic perso |
| 📱 Export PDF | Via le menu impression Android natif |
| 🔗 Partage HTML | Export du CV en fichier HTML partageable |

---

## Architecture

```
CVGenerator/
├── app/src/main/java/com/cvgenerator/
│   ├── CVGeneratorApp.kt                  # Application (Hilt + PDFBox init)
│   ├── MainActivity.kt                 # NavHost (home → result)
│   ├── data/
│   │   ├── model/
│   │   │   └── Models.kt               # Data classes (User, AuthState, JobDescription…)
│   │   └── repository/
│   │       ├── AnthropicRepository.kt  # Appels API Claude
│   │       ├── AuthRepository.kt       # Firebase Auth + liste premium
│   │       ├── DocumentRepository.kt   # Extraction texte (PDF, DOCX, TXT)
│   │       └── PreferencesRepository.kt# Stockage clé API (DataStore)
│   ├── di/
│   │   └── AppModule.kt                # Hilt DI (Firebase)
│   └── ui/
│       ├── MainViewModel.kt            # ViewModel central (Hilt)
│       ├── components/
│       │   └── Components.kt           # Composants réutilisables
│       ├── screens/
│       │   ├── HomeScreen.kt           # Écran principal
│       │   └── ResultScreen.kt         # Affichage + export du CV généré
│       └── theme/
│           ├── Color.kt                # Palette Navy/Electric Blue/Gold
│           └── Theme.kt                # MaterialTheme dark/light
└── app/
    ├── build.gradle                    # Dépendances
    ├── google-services.json            # Config Firebase (à remplacer)
    └── proguard-rules.pro
```

### Flux de données

```
[Utilisateur]
     │
     ├─ Importe LinkedIn PDF  ──► DocumentRepository.extractText()
     │                                    │
     │                               PDFBox → rawText
     │
     ├─ Importe fiche de poste ──► DocumentRepository.extractText()
     │   (fichier ou texte)              │
     │                              PDF/DOCX/TXT → rawText
     │
     ├─ Se connecte Google ──► AuthRepository.signInWithGoogle()
     │                              │
     │                         Firebase Auth → User(isPremium?)
     │
     └─ Génère le CV ──► MainViewModel.generateCV()
                              │
                    ┌─────────┴──────────┐
                    │                    │
               isPremium?            hasApiKey?
                    │                    │
              Vertex AI            AnthropicRepository
              (TODO*)            .generateCV(apiKey, ...)
                                         │
                                   API Claude Opus
                                         │
                                    HTML du CV
                                         │
                                   ResultScreen
                                   (WebView + Export)
```

> *Vertex AI : l'intégration backend (Cloud Run ou Firebase Functions) est à implémenter selon votre infrastructure GCP.

---

## Installation

### 1. Prérequis

- Android Studio Hedgehog (2023.1.1) ou supérieur
- SDK Android 26+
- Compte Firebase
- Clé API Anthropic

### 2. Firebase Setup

1. Créez un projet sur [Firebase Console](https://console.firebase.google.com)
2. Ajoutez une app Android avec le package `com.stellasecret.cvgenerator`
3. Activez **Authentication → Google Sign-In**
4. Téléchargez `google-services.json` et remplacez le fichier dans `app/`

### 3. OAuth Web Client ID

Dans `AuthRepository.kt`, remplacez :
```kotlin
.requestIdToken("YOUR_WEB_CLIENT_ID")
```
par votre Web Client ID OAuth 2.0 (disponible dans Firebase Console → Authentication → Sign-in method → Google → Web SDK configuration).

### 4. Liste des utilisateurs Premium

Dans `AuthRepository.kt`, modifiez `premiumEmails` :
```kotlin
private val premiumEmails = setOf(
    "votre@email.com",
    "autre@email.com"
)
```

> **Production** : Remplacez par un appel Firestore ou votre API backend pour gérer dynamiquement la liste.

### 5. Clé API Anthropic

Les utilisateurs non-premium saisissent leur clé directement dans l'app (stockée chiffrée via DataStore). Obtenez une clé sur [console.anthropic.com](https://console.anthropic.com).

### 6. Build

```bash
./gradlew assembleDebug
```

---

## Utilisation

1. **Exportez votre profil LinkedIn** → LinkedIn.com → Moi → Afficher le profil → Plus → Enregistrer en PDF
2. **Importez le PDF** dans l'app
3. **Ajoutez une fiche de poste** (optionnel) — fichier ou copier-coller
4. **Connectez-vous** avec Google (optionnel, pour les utilisateurs premium)
5. **Configurez votre clé API** Anthropic (si non premium)
6. Appuyez sur **"Générer mon CV"**
7. **Exportez en PDF** via le menu impression

---

## Dépendances principales

| Lib | Usage |
|---|---|
| Jetpack Compose + Material 3 | UI |
| Hilt | Injection de dépendances |
| Firebase Auth | Google OAuth |
| PDFBox Android | Extraction texte PDF |
| Apache POI | Lecture DOCX |
| OkHttp + Gson | API Anthropic |
| DataStore | Stockage clé API |
| Navigation Compose | Navigation entre écrans |

---

## Personnalisation du prompt IA

Dans `AnthropicRepository.kt`, modifiez `buildSystemPrompt()` pour adapter :
- Le style du CV (chronologique, fonctionnel, hybride)
- La langue de sortie
- Le format HTML/CSS du CV rendu
- Les sections incluses

---

## Roadmap

- [ ] Intégration Vertex AI pour utilisateurs premium (via Cloud Run)
- [ ] Historique des CVs générés (Room DB)
- [ ] Templates CV multiples
- [ ] Édition inline du CV généré
- [ ] Export DOCX
- [ ] Analyse de compatibilité profil/poste (score ATS)
