// Landing page i18n. Deliberately standalone (no build step, no bundler):
// this file is served as a plain static asset next to landing.html, so it
// carries its own tiny EN/FR table rather than sharing the SPA's Rust one.
//
// The app persists the chosen language under `cv_gen_lang` — the same key the
// Dioxus app uses — so opening the landing page and then the app keeps one
// consistent language instead of resetting it.
(function () {
  var LANG_KEY = "cv_gen_lang";

  var EN = {
    cv_nav_features: "Features",
    cv_nav_how: "How it works",
    cv_nav_usecases: "Use cases",
    cv_nav_demo: "Demo",
    cv_nav_open_app: "Open app",
    cv_hero_tag: "Local-first · Private · PDF-ready",
    cv_hero_title_1: "Tailor your CV",
    cv_hero_title_2: "to any job",
    cv_hero_sub:
      "Paste a job description. Keep your best projects. Export a clean, single-page PDF in seconds.",
    cv_hero_cta_app: "🚀 Open app",
    cv_hero_cta_tailor: "Tailor now →",
    cv_preview_name: "CV — Tailored",
    cv_preview_role: "Matches job description",
    cv_preview_match: "Match score",
    cv_preview_projects: "Relevant projects",
    cv_preview_skills: "Key skills",
    cv_preview_badge: "📄 Ready to export as PDF",
    cv_features_title: "Built for precision",
    cv_f1_title: "Job-targeted tailoring",
    cv_f1_desc:
      "Paste any job description. Get a tailored selection of projects and skills with keyword or hybrid scoring.",
    cv_f2_title: "Your choices stay",
    cv_f2_desc:
      "Manual selections are preserved when you reload or regenerate. You stay in control, never lose your edits.",
    cv_f3_title: "Saved sessions",
    cv_f3_desc:
      "Save multiple job applications. Load, rename, update in place, or duplicate as a new variant.",
    cv_f4_title: "Single-page PDF",
    cv_f4_desc:
      "Print to a clean, single-page PDF. Optimized layout, readable, and ATS-friendly.",
    cv_f5_title: "100% local",
    cv_f5_desc:
      "Your CV, job descriptions and sessions stay on your device. No tracking, no cloud by default.",
    cv_f6_title: "Import & export",
    cv_f6_desc:
      "Back up as JSON, restore when needed. Works offline and respects your workflow.",
    cv_how_title: "How it works",
    cv_step1_title: "Load your CV",
    cv_step1_desc:
      "Create or import your CV. Your projects, experiences and skills stay structured.",
    cv_step2_title: "Paste the job description",
    cv_step2_desc:
      "Go to Tailor, paste the JD. Choose Keyword or Hybrid scoring to match relevance.",
    cv_step3_title: "Refine your selection",
    cv_step3_desc:
      "Tweak projects, top projects and skills. Your manual choices are preserved across regenerations.",
    cv_step4_title: "Save & export",
    cv_step4_desc:
      "Save the session for this application, preview, then Print to PDF for a clean single-page result.",
    cv_usecases_title: "Made for every application",
    cv_uc1_title: "Job applications",
    cv_uc1_desc: "Tailor per role in seconds. Keep each variant as a saved session.",
    cv_uc2_title: "Recruiters & screenings",
    cv_uc2_desc: "Highlight the most relevant work without rewriting your entire CV.",
    cv_uc3_title: "Career pivots",
    cv_uc3_desc: "Emphasize transferable projects and skills for a new domain.",
    cv_uc4_title: "Focus & control",
    cv_uc4_desc: "Local, fast and predictable. You decide what stays and what goes.",
    cv_ethics_title: "Private by design",
    cv_ethics_desc:
      "CV Generator runs entirely in your browser. Your data never leaves your device unless you explicitly export a backup. No accounts, no tracking, no third-party data sharing.",
    cv_cta_title: "Ready to tailor your CV?",
    cv_cta_sub: "Web app • 100% local • Free • Works offline",
    cv_cta_app_btn: "🚀 Open app",
    cv_cta_tailor_btn: "🎯 Go to Tailor",
    cv_nav_cv: "Edit CV",
    cv_nav_tailor: "Tailor",
    cv_nav_sync: "Sync/Backup",
    cv_footer_copy: "Open Source • Local-first • Built for precision",
    cv_footer_github: "GitHub",
  };

  var FR = {
    cv_nav_features: "Fonctionnalités",
    cv_nav_how: "Comment ça marche",
    cv_nav_usecases: "Cas d'usage",
    cv_nav_demo: "Démo",
    cv_nav_open_app: "Ouvrir l'app",
    cv_hero_tag: "Local d'abord · Privé · Prêt pour le PDF",
    cv_hero_title_1: "Adaptez votre CV",
    cv_hero_title_2: "à n'importe quel poste",
    cv_hero_sub:
      "Collez une offre d'emploi. Gardez vos meilleurs projets. Exportez un PDF propre d'une seule page en quelques secondes.",
    cv_hero_cta_app: "🚀 Ouvrir l'app",
    cv_hero_cta_tailor: "Adapter maintenant →",
    cv_preview_name: "CV — Adapté",
    cv_preview_role: "Correspond à l'offre",
    cv_preview_match: "Score de correspondance",
    cv_preview_projects: "Projets pertinents",
    cv_preview_skills: "Compétences clés",
    cv_preview_badge: "📄 Prêt à exporter en PDF",
    cv_features_title: "Conçu pour la précision",
    cv_f1_title: "Adaptation ciblée",
    cv_f1_desc:
      "Collez n'importe quelle offre. Obtenez une sélection de projets et de compétences avec un scoring par mots-clés ou hybride.",
    cv_f2_title: "Vos choix sont conservés",
    cv_f2_desc:
      "Vos sélections manuelles sont préservées lors d'un rechargement ou d'une régénération. Vous gardez le contrôle, sans jamais perdre vos retouches.",
    cv_f3_title: "Sessions sauvegardées",
    cv_f3_desc:
      "Sauvegardez plusieurs candidatures. Rechargez, renommez, mettez à jour sur place, ou dupliquez en variante.",
    cv_f4_title: "PDF d'une page",
    cv_f4_desc:
      "Imprimez un PDF propre d'une seule page. Mise en page optimisée, lisible et compatible ATS.",
    cv_f5_title: "100% local",
    cv_f5_desc:
      "Votre CV, vos offres et vos sessions restent sur votre appareil. Aucun suivi, aucun cloud par défaut.",
    cv_f6_title: "Import & export",
    cv_f6_desc:
      "Sauvegardez en JSON et restaurez quand vous voulez. Fonctionne hors ligne et respecte votre façon de travailler.",
    cv_how_title: "Comment ça marche",
    cv_step1_title: "Chargez votre CV",
    cv_step1_desc:
      "Créez ou importez votre CV. Vos projets, expériences et compétences restent structurés.",
    cv_step2_title: "Collez l'offre d'emploi",
    cv_step2_desc:
      "Allez dans Adapter, collez l'offre. Choisissez le scoring Mots-clés ou Hybride pour mesurer la pertinence.",
    cv_step3_title: "Affinez votre sélection",
    cv_step3_desc:
      "Ajustez les projets, projets clés et compétences. Vos choix manuels sont conservés à chaque régénération.",
    cv_step4_title: "Sauvegardez & exportez",
    cv_step4_desc:
      "Sauvegardez la session pour cette candidature, prévisualisez, puis imprimez en PDF pour un résultat propre d'une page.",
    cv_usecases_title: "Pour chaque candidature",
    cv_uc1_title: "Candidatures",
    cv_uc1_desc: "Adaptez par poste en quelques secondes. Gardez chaque variante en session.",
    cv_uc2_title: "Recrutement & présélection",
    cv_uc2_desc: "Mettez en avant le travail le plus pertinent sans réécrire tout votre CV.",
    cv_uc3_title: "Reconversion",
    cv_uc3_desc: "Mettez en avant les projets et compétences transférables pour un nouveau domaine.",
    cv_uc4_title: "Maîtrise & contrôle",
    cv_uc4_desc: "Local, rapide et prévisible. Vous décidez ce qui reste et ce qui part.",
    cv_ethics_title: "Privé par conception",
    cv_ethics_desc:
      "CV Generator fonctionne entièrement dans votre navigateur. Vos données ne quittent jamais votre appareil, sauf export explicite d'une sauvegarde. Aucun compte, aucun suivi, aucun partage à des tiers.",
    cv_cta_title: "Prêt à adapter votre CV ?",
    cv_cta_sub: "App web • 100% local • Gratuit • Fonctionne hors ligne",
    cv_cta_app_btn: "🚀 Ouvrir l'application",
    cv_cta_tailor_btn: "🎯 Aller à Adapter",
    cv_nav_cv: "Modifier le CV",
    cv_nav_tailor: "Adapter",
    cv_nav_sync: "Sync/Sauvegarde",
    cv_footer_copy: "Open Source • Local d'abord • Conçu pour la précision",
    cv_footer_github: "GitHub",
  };

  function currentLang() {
    var stored = null;
    try {
      stored = localStorage.getItem(LANG_KEY);
    } catch (e) {
      stored = null;
    }
    if (stored === "en" || stored === "fr") return stored;
    // No explicit choice yet: fall back to the browser, then to French (the
    // app's own default is French, so this keeps the two pages consistent).
    var nav = (navigator.language || "").slice(0, 2).toLowerCase();
    return nav === "en" ? "en" : "fr";
  }

  function apply(lang) {
    var table = lang === "fr" ? FR : EN;
    document.documentElement.lang = lang;
    var nodes = document.querySelectorAll("[data-i18n]");
    for (var i = 0; i < nodes.length; i++) {
      var key = nodes[i].getAttribute("data-i18n");
      var text = table[key];
      // A missing key leaves the markup's own copy in place, so the page still
      // reads correctly rather than showing a raw key.
      if (text) nodes[i].textContent = text;
    }
  }

  window.toggleLang = function () {
    var next = currentLang() === "fr" ? "en" : "fr";
    try {
      localStorage.setItem(LANG_KEY, next);
    } catch (e) {
      /* private browsing: the toggle still works for this page view */
    }
    apply(next);
  };

  window.toggleTheme = function () {
    var root = document.documentElement;
    var dark = root.getAttribute("data-theme") !== "light";
    root.setAttribute("data-theme", dark ? "light" : "dark");
    var btn = document.getElementById("themeBtn");
    if (btn) btn.textContent = dark ? "🌙" : "☀️";
    try {
      localStorage.setItem("cv_gen_theme", dark ? "light" : "dark");
    } catch (e) {
      /* ignore */
    }
  };

  // Runs on load: pick the language, then wire the nav up.
  var lang = currentLang();
  apply(lang);
  try {
    var savedTheme = localStorage.getItem("cv_gen_theme");
    if (savedTheme === "light" || savedTheme === "dark") {
      document.documentElement.setAttribute("data-theme", savedTheme);
      var themeBtn = document.getElementById("themeBtn");
      if (themeBtn) themeBtn.textContent = savedTheme === "dark" ? "☀️" : "🌙";
    }
  } catch (e) {
    /* ignore */
  }
})();