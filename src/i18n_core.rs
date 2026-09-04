#![allow(dead_code)]

const LANG_KEY: &str = "cv_gen_lang";
const THEME_KEY: &str = "cv_gen_theme";

// ── Language ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    pub fn detect() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let stored = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
                .and_then(|s| s.get_item(LANG_KEY).ok())
                .flatten();
            if let Some(l) = stored {
                if l == "en" {
                    return Lang::En;
                }
                if l == "fr" {
                    return Lang::Fr;
                }
            }
            if let Some(nav) = web_sys::window().and_then(|w| w.navigator().language()) {
                if nav.starts_with("fr") {
                    return Lang::Fr;
                }
                if nav.starts_with("en") {
                    return Lang::En;
                }
            }
            Lang::Fr
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Lang::Fr
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::En => Self::Fr,
            Self::Fr => Self::En,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::En => "EN",
            Self::Fr => "FR",
        }
    }

    pub fn persist(self) {
        let s = match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        };
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
            {
                let _ = storage.set_item(LANG_KEY, s);
            }
        }
        let _ = s;
    }
}

// ── Theme ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn detect() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let stored = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
                .and_then(|s| s.get_item(THEME_KEY).ok())
                .flatten();
            if let Some(t) = stored {
                return if t == "light" {
                    Theme::Light
                } else {
                    Theme::Dark
                };
            }
            if let Some(w) = web_sys::window() {
                if let Ok(Some(mq)) = w.match_media("(prefers-color-scheme: light)") {
                    if mq.matches() {
                        return Theme::Light;
                    }
                }
            }
            Theme::Dark
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Theme::Dark
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "☀️",
            Self::Light => "🌙",
        }
    }

    pub fn persist(self) {
        let s = self.as_str();
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
            {
                let _ = storage.set_item(THEME_KEY, s);
            }
        }
        let _ = s;
    }
}

// ── Translations ──────────────────────────────────────────────────────────────

pub fn tr(key: &'static str, lang: Lang) -> &'static str {
    match lang {
        Lang::En => en(key),
        Lang::Fr => fr(key),
    }
}

fn en(key: &'static str) -> &'static str {
    match key {
        // Nav
        "nav_brand"         => "CV Gen",
        "nav_home"          => "Home",
        "nav_cv"            => "My CV",
        "nav_preview"       => "Preview",
        "nav_tailor"        => "Tailor",
        "nav_sync"          => "Sync",
        "nav_back"          => "← Home",

        // Home
        "home_title"        => "CV Generator",
        "home_subtitle"     => "Build your lifetime CV once. Generate targeted applications in seconds.",
        "home_not_filled"   => "Not filled yet",
        "home_personal"     => "Personal Info",
        "home_experience"   => "Experience",
        "home_skills"       => "Skills",
        "home_positions"    => "position(s) stored",
        "home_skills_stored"=> "skill(s) stored",
        "home_step1_title"  => "1 — Build lifetime CV",
        "home_step1_desc"   => "Enter all your experience, skills and education once. This is your personal database.",
        "home_step1_btn"    => "Edit My CV →",
        "home_step2_title"  => "2 — Preview & download",
        "home_step2_desc"   => "See your complete CV as a clean printable document. Download as PDF with one click.",
        "home_step2_btn"    => "Preview CV →",
        "home_step3_title"  => "3 — Tailor to a job",
        "home_step3_desc"   => "Paste a job description. We instantly filter and rank your CV to match it — no rewriting, no AI.",
        "home_step3_btn"    => "Tailor to JD →",
        "home_step4_title"  => "4 — Backup & Sync",
        "home_step4_desc"   => "Back up your CV to Google Drive or export it locally. Never lose your data.",
        "home_step4_btn"    => "Backup & Sync →",

        // Editor
        "ed_title"          => "My Lifetime CV",
        "ed_subtitle"       => "Your personal career database. Fill it in once, use it everywhere.",
        "ed_back"           => "← Back",
        "ed_save_finish"    => "Save & Finish →",
        "ed_save_cont"      => "Save & Continue →",
        "ed_import_pdf"     => "📄 Import PDF",
        "ed_import_pdf_err" => "PDF import error",
        "ed_step_personal"  => "Personal",
        "ed_step_experience"=> "Experience",
        "ed_step_skills"    => "Skills",
        "ed_step_education" => "Education",
        "ed_step_projects"  => "Projects",
        "ed_step_langs"     => "Languages & Certs",
        "ed_seed_from_en"   => "Fill missing French text from English",
        "ed_seed_from_fr"   => "Fill missing English text from French",
        "ed_seed_done"      => "Copied — existing translations were left untouched.",
        // Personal
        "ed_personal_title" => "Personal Information",
        "ed_fullname"       => "Full name",
        "ed_pro_title"      => "Professional title",
        "ed_email"          => "Email",
        "ed_phone"          => "Phone",
        "ed_location"       => "Location",
        "ed_linkedin"       => "LinkedIn URL",
        "ed_github"         => "GitHub URL",
        "ed_website"        => "Personal website",
        "ed_summary"        => "Professional summary",
        "ed_summary_hint"   => "2-3 sentences. Appears at the top of your CV.",
        // Experience
        "ed_exp_title"      => "Work Experience",
        "ed_exp_hint"       => "Add all positions — past and present. The tailor step picks the most relevant ones.",
        "ed_add_exp"        => "+ Add experience",
        "ed_new_position"   => "New position",
        "ed_company"        => "Company",
        "ed_role"           => "Role / Title",
        "ed_start_date"     => "Start date",
        "ed_end_date"       => "End date",
        "ed_achievements"   => "Achievements (one per line)",
        "ed_add_bullet"     => "+ Add bullet",
        "ed_tools"          => "Tools (comma-separated)",
        "ed_present"        => "Present",
        "ed_add_position"   => "Add position",
        // Experience projects
        "ed_projects"       => "Projects",
        "ed_project_name"   => "Project name",
        "ed_project_context" => "Context (challenge or goal)",
        "ed_add_context"    => "+ Add context line",
        "ed_new_project"    => "New project",
        "ed_add_project"    => "+ Add project",
        "ed_cancel"         => "Cancel",
        "ed_edit"           => "✎",
        "ed_save_changes"   => "Save",
        // Skills
        "ed_skills_title"   => "Skills",
        "ed_skills_hint"    => "Add every skill — we'll surface the most relevant ones per job description.",
        "ed_skill_name"     => "Skill name",
        "ed_skills_used"    => "Stack / Skills",
        "ed_skills_used_hint" => "Select the skills used in this role.",
        "ed_category"       => "Category",
        "ed_level"          => "Level",
        "ed_add"            => "+ Add",
        // Education
        "ed_edu_title"      => "Education",
        "ed_institution"    => "Institution",
        "ed_degree"         => "Degree",
        "ed_field"          => "Field of study",
        "ed_start_year"     => "Start year",
        "ed_end_year"       => "End year",
        "ed_add_edu"        => "+ Add education",
        // Projects
        "ed_proj_title"     => "Projects",
        "ed_proj_hint"      => "Side projects, open-source, hackathon work. Optional but helps with matching.",
        "ed_proj_name"      => "Project name",
        "ed_description"    => "Description",
        "ed_add_proj"       => "+ Add project",
        // Languages & Certs
        "ed_langs_title"    => "Languages & Certifications",
        "ed_languages"      => "Languages",
        "ed_language"       => "Language",
        "ed_certifications" => "Certifications",
        "ed_cert_name"      => "Certification name",
        "ed_issuer"         => "Issuer",
        "ed_date"           => "Date",
        "ed_add_cert"       => "+ Add certification",
        // Done
        "ed_done_title"     => "Lifetime CV saved!",
        "ed_done_desc"      => "Your data is stored locally. You can now:",
        "ed_done_preview"   => "Preview full CV →",
        "ed_done_tailor"    => "Tailor to a job description →",

        // Preview
        "pv_title"          => "CV Preview",
        "pv_subtitle"       => "Your complete lifetime CV — all experience, unfiltered.",
        "pv_edit_cv"        => "Edit CV",
        "pv_download"       => "⬇ Download PDF",
        "pv_download_hint"  => "Tip: in the print dialog, open \"More settings\" and turn off \"Headers and footers\" for a clean PDF (no title/URL/date/page-number text in the corners).",
        "pv_empty"          => "Your CV is empty.",
        "pv_fill_first"     => "Fill in your details first →",
        "pv_full_cv"        => "Full CV",

        // Tailor
        "tl_title"          => "Tailor to Job Description",
        "tl_subtitle"       => "Paste a job posting. We score your experience against it and produce a focused CV — no text changes, just smart selection.",
        "tl_empty"          => "You need to fill in your CV first.",
        "tl_build_cv"       => "Build your lifetime CV →",
        "tl_full_cv"        => "Full CV",
        "tl_job_title"      => "Job title (optional)",
        "tl_jd_label"       => "Paste the full job description",
        "tl_generate"       => "⚡ Generate Tailored CV",
        "tl_match"          => "match",
        "tl_matched"        => "✓ Matched ({} keywords)",
        "tl_missing"        => "✗ Missing ({} keywords)",
        "tl_download"       => "⬇ Download PDF",
        "tl_adjust_selection" => "Adjust your selection",
        "tl_apply_selection"  => "Apply selection",
        "tl_marker_auto"      => "automatic",
        "tl_marker_added"     => "hand-added",
        "tl_marker_removed"   => "hand-removed",
        "tl_marker_excluded"  => "not selected",
        "tl_reset_algo"       => "Restore algorithm defaults",
        "tl_clear_all"        => "Clear all",
        "tl_n_selected"       => "{} of {} projects selected",
        "tl_applied"          => "Selection applied.",
        "tl_score_note"       => "Score reflects your full CV, not this manual selection.",
        "tl_placeholder"    => "Your tailored CV will appear here after you paste a job description and click Generate.",

        // Sync
        "sy_title"          => "Backup & Sync",
        "sy_subtitle"       => "Save your CV to Google Drive or export it locally.",
        "sy_gdrive"         => "Google Drive",
        "sy_gdrive_desc"    => "Private app folder — not visible in your Drive, doesn't use storage quota.",
        "sy_sign_in"        => "Sign in with Google",
        "sy_signed_in"      => "✓ Signed in — click to sign out",
        "sy_signed_out"     => "Signed out — click again to sign in",
        "sy_signed_in_msg"  => "Signed in to Google",
        "sy_connecting"     => "Connecting…",
        "sy_backup"         => "⬆  Back up to Drive",
        "sy_restore"        => "⬇  Restore from Drive",
        "sy_working"        => "Working…",
        "sy_backup_ok"      => "CV backed up to Google Drive",
        "sy_restore_ok"     => "CV restored from Google Drive",
        "sy_config_err"     => "GOOGLE_CLIENT_ID not configured at build time",
        "sy_local"          => "Local Backup",
        "sy_local_desc"     => "No account needed. Download a JSON file and re-import any time.",
        "sy_export"         => "⬇  Export JSON",
        "sy_import"         => "⬆  Import JSON",
        "sy_json_ok"        => "JSON downloaded",
        "sy_import_ok"      => "CV imported from file",

        // Renderer section titles
        "rs_experience"     => "Experience",
        "rs_skills"         => "Skills",
        "rs_projects"       => "Projects",
        "rs_education"      => "Education",
        "rs_languages"      => "Languages",
        "rs_certifications" => "Certifications",

        _ => key,
    }
}

fn fr(key: &'static str) -> &'static str {
    match key {
        // Nav
        "nav_brand"         => "CV Gen",
        "nav_home"          => "Accueil",
        "nav_cv"            => "Mon CV",
        "nav_preview"       => "Aperçu",
        "nav_tailor"        => "Adapter",
        "nav_sync"          => "Sync",
        "nav_back"          => "← Accueil",

        // Home
        "home_title"        => "Générateur de CV",
        "home_subtitle"     => "Construisez votre CV à vie. Générez des candidatures ciblées en quelques secondes.",
        "home_not_filled"   => "Non renseigné",
        "home_personal"     => "Informations",
        "home_experience"   => "Expérience",
        "home_skills"       => "Compétences",
        "home_positions"    => "poste(s) enregistré(s)",
        "home_skills_stored"=> "compétence(s) enregistrée(s)",
        "home_step1_title"  => "1 — Construire le CV",
        "home_step1_desc"   => "Saisissez une fois toute votre expérience, compétences et formation. C'est votre base de données personnelle.",
        "home_step1_btn"    => "Éditer mon CV →",
        "home_step2_title"  => "2 — Aperçu et téléchargement",
        "home_step2_desc"   => "Consultez votre CV complet sous forme de document propre et imprimable. Téléchargez en PDF en un clic.",
        "home_step2_btn"    => "Aperçu du CV →",
        "home_step3_title"  => "3 — Adapter à une offre",
        "home_step3_desc"   => "Collez une fiche de poste. Nous filtrons et classons instantanément votre CV — sans réécriture, sans IA.",
        "home_step3_btn"    => "Adapter à une fiche →",
        "home_step4_title"  => "4 — Sauvegarde et synchronisation",
        "home_step4_desc"   => "Sauvegardez votre CV sur Google Drive ou exportez-le localement. Ne perdez jamais vos données.",
        "home_step4_btn"    => "Sauvegarde et Sync →",

        // Editor
        "ed_title"          => "Mon CV à vie",
        "ed_subtitle"       => "Votre base de données de carrière. Saisissez une fois, utilisez partout.",
        "ed_back"           => "← Retour",
        "ed_save_finish"    => "Enregistrer et terminer →",
        "ed_save_cont"      => "Enregistrer et continuer →",
        "ed_import_pdf"     => "📄 Importer un PDF",
        "ed_import_pdf_err" => "Erreur d'import PDF",
        "ed_step_personal"  => "Personnel",
        "ed_step_experience"=> "Expérience",
        "ed_step_skills"    => "Compétences",
        "ed_step_education" => "Formation",
        "ed_step_projects"  => "Projets",
        "ed_step_langs"     => "Langues & Certs",
        "ed_seed_from_en"   => "Compléter le français manquant depuis l'anglais",
        "ed_seed_from_fr"   => "Compléter l'anglais manquant depuis le français",
        "ed_seed_done"      => "Copié — les traductions déjà saisies n'ont pas été modifiées.",
        // Personal
        "ed_personal_title" => "Informations personnelles",
        "ed_fullname"       => "Nom complet",
        "ed_pro_title"      => "Titre professionnel",
        "ed_email"          => "E-mail",
        "ed_phone"          => "Téléphone",
        "ed_location"       => "Localisation",
        "ed_linkedin"       => "URL LinkedIn",
        "ed_github"         => "URL GitHub",
        "ed_website"        => "Site web personnel",
        "ed_summary"        => "Résumé professionnel",
        "ed_summary_hint"   => "2-3 phrases. Apparaît en haut de votre CV.",
        // Experience
        "ed_exp_title"      => "Expérience professionnelle",
        "ed_exp_hint"       => "Ajoutez tous les postes — passés et présents. L'étape d'adaptation sélectionne les plus pertinents.",
        "ed_add_exp"        => "+ Ajouter une expérience",
        "ed_new_position"   => "Nouveau poste",
        "ed_company"        => "Entreprise",
        "ed_role"           => "Poste / Intitulé",
        "ed_start_date"     => "Date de début",
        "ed_end_date"       => "Date de fin",
        "ed_achievements"   => "Réalisations (une par ligne)",
        "ed_add_bullet"     => "+ Ajouter une ligne",
        "ed_tools"          => "Outils (séparés par des virgules)",
        "ed_present"        => "Présent",
        "ed_add_position"   => "Ajouter le poste",
        "ed_cancel"         => "Annuler",
        "ed_edit"           => "✎",
        "ed_save_changes"   => "Enregistrer",
        // Experience projects
        "ed_projects"       => "Projets",
        "ed_project_name"   => "Nom du projet",
        "ed_project_context" => "Contexte (défi ou objectif)",
        "ed_add_context"    => "+ Ajouter une ligne de contexte",
        "ed_new_project"    => "Nouveau projet",
        "ed_add_project"    => "+ Ajouter un projet",
        // Skills
        "ed_skills_title"   => "Compétences",
        "ed_skills_hint"    => "Ajoutez chaque compétence — nous mettrons en avant les plus pertinentes pour chaque offre.",
        "ed_skill_name"     => "Nom de la compétence",
        "ed_skills_used"    => "Stack / Compétences",
        "ed_skills_used_hint" => "Sélectionnez les compétences utilisées dans ce poste.",
        "ed_category"       => "Catégorie",
        "ed_level"          => "Niveau",
        "ed_add"            => "+ Ajouter",
        // Education
        "ed_edu_title"      => "Formation",
        "ed_institution"    => "Établissement",
        "ed_degree"         => "Diplôme",
        "ed_field"          => "Domaine d'études",
        "ed_start_year"     => "Année de début",
        "ed_end_year"       => "Année de fin",
        "ed_add_edu"        => "+ Ajouter une formation",
        // Projects
        "ed_proj_title"     => "Projets",
        "ed_proj_hint"      => "Projets personnels, open-source, hackathons. Optionnel mais aide à l'adaptation.",
        "ed_proj_name"      => "Nom du projet",
        "ed_description"    => "Description",
        "ed_add_proj"       => "+ Ajouter un projet",
        // Languages & Certs
        "ed_langs_title"    => "Langues & Certifications",
        "ed_languages"      => "Langues",
        "ed_language"       => "Langue",
        "ed_certifications" => "Certifications",
        "ed_cert_name"      => "Nom de la certification",
        "ed_issuer"         => "Organisme",
        "ed_date"           => "Date",
        "ed_add_cert"       => "+ Ajouter une certification",
        // Done
        "ed_done_title"     => "CV enregistré !",
        "ed_done_desc"      => "Vos données sont stockées localement. Vous pouvez maintenant :",
        "ed_done_preview"   => "Aperçu du CV complet →",
        "ed_done_tailor"    => "Adapter à une fiche de poste →",

        // Preview
        "pv_title"          => "Aperçu du CV",
        "pv_subtitle"       => "Votre CV complet — toute l'expérience, sans filtre.",
        "pv_edit_cv"        => "Modifier le CV",
        "pv_download"       => "⬇ Télécharger PDF",
        "pv_download_hint"  => "Astuce : dans la boîte de dialogue d'impression, ouvrez « Plus de paramètres » et désactivez « En-têtes et pieds de page » pour un PDF propre (sans titre/URL/date/numéro de page dans les coins).",
        "pv_empty"          => "Votre CV est vide.",
        "pv_fill_first"     => "Renseignez vos informations →",
        "pv_full_cv"        => "CV complet",

        // Tailor
        "tl_title"          => "Adapter à une fiche de poste",
        "tl_subtitle"       => "Collez une offre d'emploi. Nous évaluons votre expérience et produisons un CV ciblé — sans modification de texte, juste une sélection intelligente.",
        "tl_empty"          => "Vous devez d'abord compléter votre CV.",
        "tl_build_cv"       => "Construire mon CV →",
        "tl_full_cv"        => "CV complet",
        "tl_job_title"      => "Intitulé du poste (optionnel)",
        "tl_jd_label"       => "Collez la fiche de poste complète",
        "tl_generate"       => "⚡ Générer le CV adapté",
        "tl_match"          => "correspondance",
        "tl_matched"        => "✓ Trouvés ({} mots-clés)",
        "tl_missing"        => "✗ Manquants ({} mots-clés)",
        "tl_download"       => "⬇ Télécharger PDF",
        "tl_adjust_selection" => "Ajustez votre sélection",
        "tl_apply_selection"  => "Appliquer la sélection",
        "tl_marker_auto"      => "automatique",
        "tl_marker_added"     => "ajouté",
        "tl_marker_removed"   => "retiré",
        "tl_marker_excluded"  => "non sélectionné",
        "tl_reset_algo"       => "Restaurer la sélection automatique",
        "tl_clear_all"        => "Tout décocher",
        "tl_n_selected"       => "{} sur {} projets sélectionnés",
        "tl_applied"          => "Sélection appliquée.",
        "tl_score_note"       => "Le score reflète votre CV complet, pas cette sélection manuelle.",
        "tl_placeholder"    => "Votre CV adapté apparaîtra ici après avoir collé une fiche de poste et cliqué Générer.",

        // Sync
        "sy_title"          => "Sauvegarde et synchronisation",
        "sy_subtitle"       => "Sauvegardez votre CV sur Google Drive ou exportez-le localement.",
        "sy_gdrive"         => "Google Drive",
        "sy_gdrive_desc"    => "Dossier privé de l'application — pas visible dans Drive, sans quota de stockage.",
        "sy_sign_in"        => "Se connecter avec Google",
        "sy_signed_in"      => "✓ Connecté — cliquer pour se déconnecter",
        "sy_signed_out"     => "Déconnecté — cliquer pour se reconnecter",
        "sy_signed_in_msg"  => "Connecté à Google",
        "sy_connecting"     => "Connexion…",
        "sy_backup"         => "⬆  Sauvegarder sur Drive",
        "sy_restore"        => "⬇  Restaurer depuis Drive",
        "sy_working"        => "Traitement…",
        "sy_backup_ok"      => "CV sauvegardé sur Google Drive",
        "sy_restore_ok"     => "CV restauré depuis Google Drive",
        "sy_config_err"     => "GOOGLE_CLIENT_ID non configuré à la compilation",
        "sy_local"          => "Sauvegarde locale",
        "sy_local_desc"     => "Pas de compte nécessaire. Téléchargez un fichier JSON et réimportez-le à tout moment.",
        "sy_export"         => "⬇  Exporter JSON",
        "sy_import"         => "⬆  Importer JSON",
        "sy_json_ok"        => "JSON téléchargé",
        "sy_import_ok"      => "CV importé depuis le fichier",

        // Renderer section titles
        "rs_experience"     => "Expérience",
        "rs_skills"         => "Compétences",
        "rs_projects"       => "Projets",
        "rs_education"      => "Formation",
        "rs_languages"      => "Langues",
        "rs_certifications" => "Certifications",

        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_detect_native_returns_fr() {
        assert_eq!(Lang::detect(), Lang::Fr);
    }

    #[test]
    fn lang_toggle_en_to_fr() {
        assert_eq!(Lang::En.toggle(), Lang::Fr);
    }

    #[test]
    fn lang_toggle_fr_to_en() {
        assert_eq!(Lang::Fr.toggle(), Lang::En);
    }

    #[test]
    fn lang_toggle_roundtrip() {
        let lang = Lang::En;
        assert_eq!(lang.toggle().toggle(), lang);
    }

    #[test]
    fn lang_label_en() {
        assert_eq!(Lang::En.label(), "EN");
    }

    #[test]
    fn lang_label_fr() {
        assert_eq!(Lang::Fr.label(), "FR");
    }

    #[test]
    fn lang_persist_native_no_panic() {
        Lang::En.persist();
        Lang::Fr.persist();
    }

    #[test]
    fn lang_copy() {
        let a = Lang::En;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn theme_detect_native_returns_dark() {
        assert_eq!(Theme::detect(), Theme::Dark);
    }

    #[test]
    fn theme_toggle_dark_to_light() {
        assert_eq!(Theme::Dark.toggle(), Theme::Light);
    }

    #[test]
    fn theme_toggle_light_to_dark() {
        assert_eq!(Theme::Light.toggle(), Theme::Dark);
    }

    #[test]
    fn theme_toggle_roundtrip() {
        let t = Theme::Dark;
        assert_eq!(t.toggle().toggle(), t);
    }

    #[test]
    fn theme_as_str_dark() {
        assert_eq!(Theme::Dark.as_str(), "dark");
    }

    #[test]
    fn theme_as_str_light() {
        assert_eq!(Theme::Light.as_str(), "light");
    }

    #[test]
    fn theme_label_dark() {
        assert_eq!(Theme::Dark.label(), "☀️");
    }

    #[test]
    fn theme_label_light() {
        assert_eq!(Theme::Light.label(), "🌙");
    }

    #[test]
    fn theme_persist_native_no_panic() {
        Theme::Dark.persist();
        Theme::Light.persist();
    }

    #[test]
    fn theme_copy() {
        let a = Theme::Light;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn tr_en_returns_english() {
        assert_eq!(tr("nav_home", Lang::En), "Home");
        assert_eq!(tr("home_title", Lang::En), "CV Generator");
        assert_eq!(tr("ed_title", Lang::En), "My Lifetime CV");
    }

    #[test]
    fn tr_fr_returns_french() {
        assert_eq!(tr("nav_home", Lang::Fr), "Accueil");
        assert_eq!(tr("home_title", Lang::Fr), "Générateur de CV");
        assert_eq!(tr("ed_title", Lang::Fr), "Mon CV à vie");
    }

    #[test]
    fn tr_unknown_key_returns_key() {
        assert_eq!(tr("nonexistent_key", Lang::En), "nonexistent_key");
        assert_eq!(tr("nonexistent_key", Lang::Fr), "nonexistent_key");
    }

    #[test]
    fn tr_nav_brand_same_in_both_langs() {
        assert_eq!(tr("nav_brand", Lang::En), tr("nav_brand", Lang::Fr));
        assert_eq!(tr("nav_brand", Lang::En), "CV Gen");
    }

    #[test]
    fn tr_all_nav_keys_exist() {
        let nav_keys = [
            "nav_brand",
            "nav_home",
            "nav_cv",
            "nav_preview",
            "nav_tailor",
            "nav_sync",
            "nav_back",
        ];
        for key in nav_keys {
            assert_ne!(
                tr(key, Lang::En),
                key,
                "EN nav key '{}' returned itself",
                key
            );
            assert_ne!(
                tr(key, Lang::Fr),
                key,
                "FR nav key '{}' returned itself",
                key
            );
        }
    }

    #[test]
    fn tr_all_editor_keys_exist() {
        let keys = [
            "ed_title",
            "ed_step_personal",
            "ed_step_experience",
            "ed_step_skills",
            "ed_step_education",
            "ed_step_projects",
            "ed_step_langs",
            "ed_seed_from_en",
            "ed_seed_from_fr",
            "ed_seed_done",
        ];
        for key in keys {
            assert_ne!(tr(key, Lang::En), key);
            assert_ne!(tr(key, Lang::Fr), key);
        }
    }

    #[test]
    fn tr_all_sync_keys_exist() {
        let keys = [
            "sy_title",
            "sy_gdrive",
            "sy_sign_in",
            "sy_backup",
            "sy_restore",
            "sy_export",
            "sy_import",
        ];
        for key in keys {
            assert_ne!(tr(key, Lang::En), key);
            assert_ne!(tr(key, Lang::Fr), key);
        }
    }

    #[test]
    fn tr_en_and_fr_never_same_except_brand() {
        let nav_keys = ["nav_home", "nav_cv", "nav_preview", "nav_tailor"];
        for key in nav_keys {
            assert_ne!(
                tr(key, Lang::En),
                tr(key, Lang::Fr),
                "key '{}' is same in both langs",
                key
            );
        }
        // nav_sync is intentionally identical in EN/FR ("Sync" is an international loanword)
    }

    #[test]
    fn all_en_translations_present() {
        let cases: Vec<(&str, &str)> = vec![
            ("nav_brand", "CV Gen"),
            ("nav_home", "Home"),
            ("nav_cv", "My CV"),
            ("nav_preview", "Preview"),
            ("nav_tailor", "Tailor"),
            ("nav_sync", "Sync"),
            ("nav_back", "← Home"),
            ("home_title", "CV Generator"),
            ("home_subtitle", "Build your lifetime CV once. Generate targeted applications in seconds."),
            ("home_not_filled", "Not filled yet"),
            ("home_personal", "Personal Info"),
            ("home_experience", "Experience"),
            ("home_skills", "Skills"),
            ("home_positions", "position(s) stored"),
            ("home_skills_stored", "skill(s) stored"),
            ("home_step1_title", "1 — Build lifetime CV"),
            ("home_step1_desc", "Enter all your experience, skills and education once. This is your personal database."),
            ("home_step1_btn", "Edit My CV →"),
            ("home_step2_title", "2 — Preview & download"),
            ("home_step2_desc", "See your complete CV as a clean printable document. Download as PDF with one click."),
            ("home_step2_btn", "Preview CV →"),
            ("home_step3_title", "3 — Tailor to a job"),
            ("home_step3_desc", "Paste a job description. We instantly filter and rank your CV to match it — no rewriting, no AI."),
            ("home_step3_btn", "Tailor to JD →"),
            ("home_step4_title", "4 — Backup & Sync"),
            ("home_step4_desc", "Back up your CV to Google Drive or export it locally. Never lose your data."),
            ("home_step4_btn", "Backup & Sync →"),
            ("ed_title", "My Lifetime CV"),
            ("ed_subtitle", "Your personal career database. Fill it in once, use it everywhere."),
            ("ed_back", "← Back"),
            ("ed_save_finish", "Save & Finish →"),
            ("ed_save_cont", "Save & Continue →"),
            ("ed_import_pdf", "📄 Import PDF"),
            ("ed_import_pdf_err", "PDF import error"),
            ("ed_step_personal", "Personal"),
            ("ed_step_experience", "Experience"),
            ("ed_step_skills", "Skills"),
            ("ed_step_education", "Education"),
            ("ed_step_projects", "Projects"),
            ("ed_step_langs", "Languages & Certs"),
            ("ed_seed_from_en", "Fill missing French text from English"),
            ("ed_seed_from_fr", "Fill missing English text from French"),
            ("ed_seed_done", "Copied — existing translations were left untouched."),
            ("ed_personal_title", "Personal Information"),
            ("ed_fullname", "Full name"),
            ("ed_pro_title", "Professional title"),
            ("ed_email", "Email"),
            ("ed_phone", "Phone"),
            ("ed_location", "Location"),
            ("ed_linkedin", "LinkedIn URL"),
            ("ed_github", "GitHub URL"),
            ("ed_website", "Personal website"),
            ("ed_summary", "Professional summary"),
            ("ed_summary_hint", "2-3 sentences. Appears at the top of your CV."),
            ("ed_exp_title", "Work Experience"),
            ("ed_exp_hint", "Add all positions — past and present. The tailor step picks the most relevant ones."),
            ("ed_add_exp", "+ Add experience"),
            ("ed_new_position", "New position"),
            ("ed_company", "Company"),
            ("ed_role", "Role / Title"),
            ("ed_start_date", "Start date"),
            ("ed_end_date", "End date"),
            ("ed_achievements", "Achievements (one per line)"),
            ("ed_add_bullet", "+ Add bullet"),
            ("ed_tools", "Tools (comma-separated)"),
            ("ed_present", "Present"),
            ("ed_add_position", "Add position"),
            ("ed_projects", "Projects"),
            ("ed_project_name", "Project name"),
            ("ed_project_context", "Context (challenge or goal)"),
            ("ed_add_context", "+ Add context line"),
            ("ed_new_project", "New project"),
            ("ed_add_project", "+ Add project"),
            ("ed_cancel", "Cancel"),
            ("ed_edit", "✎"),
            ("ed_save_changes", "Save"),
            ("ed_skills_title", "Skills"),
            ("ed_skills_hint", "Add every skill — we'll surface the most relevant ones per job description."),
            ("ed_skill_name", "Skill name"),
            ("ed_skills_used", "Stack / Skills"),
            ("ed_skills_used_hint", "Select the skills used in this role."),
            ("ed_category", "Category"),
            ("ed_level", "Level"),
            ("ed_add", "+ Add"),
            ("ed_edu_title", "Education"),
            ("ed_institution", "Institution"),
            ("ed_degree", "Degree"),
            ("ed_field", "Field of study"),
            ("ed_start_year", "Start year"),
            ("ed_end_year", "End year"),
            ("ed_add_edu", "+ Add education"),
            ("ed_proj_title", "Projects"),
            ("ed_proj_hint", "Side projects, open-source, hackathon work. Optional but helps with matching."),
            ("ed_proj_name", "Project name"),
            ("ed_description", "Description"),
            ("ed_add_proj", "+ Add project"),
            ("ed_langs_title", "Languages & Certifications"),
            ("ed_languages", "Languages"),
            ("ed_language", "Language"),
            ("ed_certifications", "Certifications"),
            ("ed_cert_name", "Certification name"),
            ("ed_issuer", "Issuer"),
            ("ed_date", "Date"),
            ("ed_add_cert", "+ Add certification"),
            ("ed_done_title", "Lifetime CV saved!"),
            ("ed_done_desc", "Your data is stored locally. You can now:"),
            ("ed_done_preview", "Preview full CV →"),
            ("ed_done_tailor", "Tailor to a job description →"),
            ("pv_title", "CV Preview"),
            ("pv_subtitle", "Your complete lifetime CV — all experience, unfiltered."),
            ("pv_edit_cv", "Edit CV"),
            ("pv_download", "⬇ Download PDF"),
            ("pv_download_hint", "Tip: in the print dialog, open \"More settings\" and turn off \"Headers and footers\" for a clean PDF (no title/URL/date/page-number text in the corners)."),
            ("pv_empty", "Your CV is empty."),
            ("pv_fill_first", "Fill in your details first →"),
            ("pv_full_cv", "Full CV"),
            ("tl_title", "Tailor to Job Description"),
            ("tl_subtitle", "Paste a job posting. We score your experience against it and produce a focused CV — no text changes, just smart selection."),
            ("tl_empty", "You need to fill in your CV first."),
            ("tl_build_cv", "Build your lifetime CV →"),
            ("tl_full_cv", "Full CV"),
            ("tl_job_title", "Job title (optional)"),
            ("tl_jd_label", "Paste the full job description"),
            ("tl_generate", "⚡ Generate Tailored CV"),
            ("tl_match", "match"),
            ("tl_matched", "✓ Matched ({} keywords)"),
            ("tl_missing", "✗ Missing ({} keywords)"),
            ("tl_download", "⬇ Download PDF"),
            ("tl_adjust_selection", "Adjust your selection"),
            ("tl_apply_selection", "Apply selection"),
            ("tl_marker_auto", "automatic"),
            ("tl_marker_added", "hand-added"),
            ("tl_marker_removed", "hand-removed"),
            ("tl_marker_excluded", "not selected"),
            ("tl_reset_algo", "Restore algorithm defaults"),
            ("tl_clear_all", "Clear all"),
            ("tl_n_selected", "{} of {} projects selected"),
            ("tl_applied", "Selection applied."),
            ("tl_score_note", "Score reflects your full CV, not this manual selection."),
            ("tl_placeholder", "Your tailored CV will appear here after you paste a job description and click Generate."),
            ("sy_title", "Backup & Sync"),
            ("sy_subtitle", "Save your CV to Google Drive or export it locally."),
            ("sy_gdrive", "Google Drive"),
            ("sy_gdrive_desc", "Private app folder — not visible in your Drive, doesn't use storage quota."),
            ("sy_sign_in", "Sign in with Google"),
            ("sy_signed_in", "✓ Signed in — click to sign out"),
            ("sy_signed_out", "Signed out — click again to sign in"),
            ("sy_signed_in_msg", "Signed in to Google"),
            ("sy_connecting", "Connecting…"),
            ("sy_backup", "⬆  Back up to Drive"),
            ("sy_restore", "⬇  Restore from Drive"),
            ("sy_working", "Working…"),
            ("sy_backup_ok", "CV backed up to Google Drive"),
            ("sy_restore_ok", "CV restored from Google Drive"),
            ("sy_config_err", "GOOGLE_CLIENT_ID not configured at build time"),
            ("sy_local", "Local Backup"),
            ("sy_local_desc", "No account needed. Download a JSON file and re-import any time."),
            ("sy_export", "⬇  Export JSON"),
            ("sy_import", "⬆  Import JSON"),
            ("sy_json_ok", "JSON downloaded"),
            ("sy_import_ok", "CV imported from file"),
            ("rs_experience", "Experience"),
            ("rs_skills", "Skills"),
            ("rs_projects", "Projects"),
            ("rs_education", "Education"),
            ("rs_languages", "Languages"),
            ("rs_certifications", "Certifications"),
        ];
        for (key, expected) in cases {
            assert_eq!(en(key), expected, "en({}) failed", key);
        }
    }

    #[test]
    fn all_fr_translations_present() {
        let cases: Vec<(&str, &str)> = vec![
            ("nav_brand", "CV Gen"),
            ("nav_home", "Accueil"),
            ("nav_cv", "Mon CV"),
            ("nav_preview", "Aperçu"),
            ("nav_tailor", "Adapter"),
            ("nav_sync", "Sync"),
            ("nav_back", "← Accueil"),
            ("home_title", "Générateur de CV"),
            ("home_subtitle", "Construisez votre CV à vie. Générez des candidatures ciblées en quelques secondes."),
            ("home_not_filled", "Non renseigné"),
            ("home_personal", "Informations"),
            ("home_experience", "Expérience"),
            ("home_skills", "Compétences"),
            ("home_positions", "poste(s) enregistré(s)"),
            ("home_skills_stored", "compétence(s) enregistrée(s)"),
            ("home_step1_title", "1 — Construire le CV"),
            ("home_step1_desc", "Saisissez une fois toute votre expérience, compétences et formation. C'est votre base de données personnelle."),
            ("home_step1_btn", "Éditer mon CV →"),
            ("home_step2_title", "2 — Aperçu et téléchargement"),
            ("home_step2_desc", "Consultez votre CV complet sous forme de document propre et imprimable. Téléchargez en PDF en un clic."),
            ("home_step2_btn", "Aperçu du CV →"),
            ("home_step3_title", "3 — Adapter à une offre"),
            ("home_step3_desc", "Collez une fiche de poste. Nous filtrons et classons instantanément votre CV — sans réécriture, sans IA."),
            ("home_step3_btn", "Adapter à une fiche →"),
            ("home_step4_title", "4 — Sauvegarde et synchronisation"),
            ("home_step4_desc", "Sauvegardez votre CV sur Google Drive ou exportez-le localement. Ne perdez jamais vos données."),
            ("home_step4_btn", "Sauvegarde et Sync →"),
            ("ed_title", "Mon CV à vie"),
            ("ed_subtitle", "Votre base de données de carrière. Saisissez une fois, utilisez partout."),
            ("ed_back", "← Retour"),
            ("ed_save_finish", "Enregistrer et terminer →"),
            ("ed_save_cont", "Enregistrer et continuer →"),
            ("ed_import_pdf", "📄 Importer un PDF"),
            ("ed_import_pdf_err", "Erreur d'import PDF"),
            ("ed_step_personal", "Personnel"),
            ("ed_step_experience", "Expérience"),
            ("ed_step_skills", "Compétences"),
            ("ed_step_education", "Formation"),
            ("ed_step_projects", "Projets"),
            ("ed_step_langs", "Langues & Certs"),
            ("ed_seed_from_en", "Compléter le français manquant depuis l'anglais"),
            ("ed_seed_from_fr", "Compléter l'anglais manquant depuis le français"),
            ("ed_seed_done", "Copié — les traductions déjà saisies n'ont pas été modifiées."),
            ("ed_personal_title", "Informations personnelles"),
            ("ed_fullname", "Nom complet"),
            ("ed_pro_title", "Titre professionnel"),
            ("ed_email", "E-mail"),
            ("ed_phone", "Téléphone"),
            ("ed_location", "Localisation"),
            ("ed_linkedin", "URL LinkedIn"),
            ("ed_github", "URL GitHub"),
            ("ed_website", "Site web personnel"),
            ("ed_summary", "Résumé professionnel"),
            ("ed_summary_hint", "2-3 phrases. Apparaît en haut de votre CV."),
            ("ed_exp_title", "Expérience professionnelle"),
            ("ed_exp_hint", "Ajoutez tous les postes — passés et présents. L'étape d'adaptation sélectionne les plus pertinents."),
            ("ed_add_exp", "+ Ajouter une expérience"),
            ("ed_new_position", "Nouveau poste"),
            ("ed_company", "Entreprise"),
            ("ed_role", "Poste / Intitulé"),
            ("ed_start_date", "Date de début"),
            ("ed_end_date", "Date de fin"),
            ("ed_achievements", "Réalisations (une par ligne)"),
            ("ed_add_bullet", "+ Ajouter une ligne"),
            ("ed_tools", "Outils (séparés par des virgules)"),
            ("ed_present", "Présent"),
            ("ed_add_position", "Ajouter le poste"),
            ("ed_cancel", "Annuler"),
            ("ed_edit", "✎"),
            ("ed_save_changes", "Enregistrer"),
            ("ed_projects", "Projets"),
            ("ed_project_name", "Nom du projet"),
            ("ed_project_context", "Contexte (défi ou objectif)"),
            ("ed_add_context", "+ Ajouter une ligne de contexte"),
            ("ed_new_project", "Nouveau projet"),
            ("ed_add_project", "+ Ajouter un projet"),
            ("ed_skills_title", "Compétences"),
            ("ed_skills_hint", "Ajoutez chaque compétence — nous mettrons en avant les plus pertinentes pour chaque offre."),
            ("ed_skill_name", "Nom de la compétence"),
            ("ed_skills_used", "Stack / Compétences"),
            ("ed_skills_used_hint", "Sélectionnez les compétences utilisées dans ce poste."),
            ("ed_category", "Catégorie"),
            ("ed_level", "Niveau"),
            ("ed_add", "+ Ajouter"),
            ("ed_edu_title", "Formation"),
            ("ed_institution", "Établissement"),
            ("ed_degree", "Diplôme"),
            ("ed_field", "Domaine d'études"),
            ("ed_start_year", "Année de début"),
            ("ed_end_year", "Année de fin"),
            ("ed_add_edu", "+ Ajouter une formation"),
            ("ed_proj_title", "Projets"),
            ("ed_proj_hint", "Projets personnels, open-source, hackathons. Optionnel mais aide à l'adaptation."),
            ("ed_proj_name", "Nom du projet"),
            ("ed_description", "Description"),
            ("ed_add_proj", "+ Ajouter un projet"),
            ("ed_langs_title", "Langues & Certifications"),
            ("ed_languages", "Langues"),
            ("ed_language", "Langue"),
            ("ed_certifications", "Certifications"),
            ("ed_cert_name", "Nom de la certification"),
            ("ed_issuer", "Organisme"),
            ("ed_date", "Date"),
            ("ed_add_cert", "+ Ajouter une certification"),
            ("ed_done_title", "CV enregistré !"),
            ("ed_done_desc", "Vos données sont stockées localement. Vous pouvez maintenant :"),
            ("ed_done_preview", "Aperçu du CV complet →"),
            ("ed_done_tailor", "Adapter à une fiche de poste →"),
            ("pv_title", "Aperçu du CV"),
            ("pv_subtitle", "Votre CV complet — toute l'expérience, sans filtre."),
            ("pv_edit_cv", "Modifier le CV"),
            ("pv_download", "⬇ Télécharger PDF"),
            ("pv_download_hint", "Astuce : dans la boîte de dialogue d'impression, ouvrez « Plus de paramètres » et désactivez « En-têtes et pieds de page » pour un PDF propre (sans titre/URL/date/numéro de page dans les coins)."),
            ("pv_empty", "Votre CV est vide."),
            ("pv_fill_first", "Renseignez vos informations →"),
            ("pv_full_cv", "CV complet"),
            ("tl_title", "Adapter à une fiche de poste"),
            ("tl_subtitle", "Collez une offre d'emploi. Nous évaluons votre expérience et produisons un CV ciblé — sans modification de texte, juste une sélection intelligente."),
            ("tl_empty", "Vous devez d'abord compléter votre CV."),
            ("tl_build_cv", "Construire mon CV →"),
            ("tl_full_cv", "CV complet"),
            ("tl_job_title", "Intitulé du poste (optionnel)"),
            ("tl_jd_label", "Collez la fiche de poste complète"),
            ("tl_generate", "⚡ Générer le CV adapté"),
            ("tl_match", "correspondance"),
            ("tl_matched", "✓ Trouvés ({} mots-clés)"),
            ("tl_missing", "✗ Manquants ({} mots-clés)"),
            ("tl_download", "⬇ Télécharger PDF"),
            ("tl_adjust_selection", "Ajustez votre sélection"),
            ("tl_apply_selection", "Appliquer la sélection"),
            ("tl_marker_auto", "automatique"),
            ("tl_marker_added", "ajouté"),
            ("tl_marker_removed", "retiré"),
            ("tl_marker_excluded", "non sélectionné"),
            ("tl_reset_algo", "Restaurer la sélection automatique"),
            ("tl_clear_all", "Tout décocher"),
            ("tl_n_selected", "{} sur {} projets sélectionnés"),
            ("tl_applied", "Sélection appliquée."),
            ("tl_score_note", "Le score reflète votre CV complet, pas cette sélection manuelle."),
            ("tl_placeholder", "Votre CV adapté apparaîtra ici après avoir collé une fiche de poste et cliqué Générer."),
            ("sy_title", "Sauvegarde et synchronisation"),
            ("sy_subtitle", "Sauvegardez votre CV sur Google Drive ou exportez-le localement."),
            ("sy_gdrive", "Google Drive"),
            ("sy_gdrive_desc", "Dossier privé de l'application — pas visible dans Drive, sans quota de stockage."),
            ("sy_sign_in", "Se connecter avec Google"),
            ("sy_signed_in", "✓ Connecté — cliquer pour se déconnecter"),
            ("sy_signed_out", "Déconnecté — cliquer pour se reconnecter"),
            ("sy_signed_in_msg", "Connecté à Google"),
            ("sy_connecting", "Connexion…"),
            ("sy_backup", "⬆  Sauvegarder sur Drive"),
            ("sy_restore", "⬇  Restaurer depuis Drive"),
            ("sy_working", "Traitement…"),
            ("sy_backup_ok", "CV sauvegardé sur Google Drive"),
            ("sy_restore_ok", "CV restauré depuis Google Drive"),
            ("sy_config_err", "GOOGLE_CLIENT_ID non configuré à la compilation"),
            ("sy_local", "Sauvegarde locale"),
            ("sy_local_desc", "Pas de compte nécessaire. Téléchargez un fichier JSON et réimportez-le à tout moment."),
            ("sy_export", "⬇  Exporter JSON"),
            ("sy_import", "⬆  Importer JSON"),
            ("sy_json_ok", "JSON téléchargé"),
            ("sy_import_ok", "CV importé depuis le fichier"),
            ("rs_experience", "Expérience"),
            ("rs_skills", "Compétences"),
            ("rs_projects", "Projets"),
            ("rs_education", "Formation"),
            ("rs_languages", "Langues"),
            ("rs_certifications", "Certifications"),
        ];
        for (key, expected) in cases {
            assert_eq!(fr(key), expected, "fr({}) failed", key);
        }
    }
}
