// Demo page i18n. Standalone for the same reason as js/landing.js: there is no
// build step, this ships as a plain static asset, so it carries its own EN/FR
// table rather than sharing the SPA's Rust one.
//
// The localStorage keys are the app's own (`cv_gen_lang`, `cv_gen_theme`) and
// landing.js uses them too, so the language and theme stay put when moving
// between the landing page, this demo, and the app.
(function () {
  var LANG_KEY = "cv_gen_lang";
  var THEME_KEY = "cv_gen_theme";

  var EN = {
    cv_nav_features: "Features",
    cv_nav_how: "How it works",
    cv_nav_demo: "Demo",
    cv_nav_open_app: "Open app",

    cv_demo_notice:
      "This is a static preview of the app with a fictional CV and job description. Nothing here is real, nothing is stored, and nothing runs in your browser.",
    cv_demo_title: "Tailoring a CV, end to end",
    cv_demo_sub:
      "Below is what one run looks like: the job description on the left, the score and the selection it produced on the right.",

    cv_demo_field_title: "Job title",
    cv_demo_field_jd: "Job description",
    cv_demo_field_mode: "Matching mode",
    cv_demo_mode_keyword: "Keyword",
    cv_demo_mode_embedding: "Embedding",
    cv_demo_mode_hybrid: "Hybrid",
    cv_demo_mode_hint:
      "Hybrid blends keyword overlap with semantic similarity, which is why it wins on job descriptions that avoid the exact words on your CV.",
    cv_demo_generate: "Generate",
    cv_demo_static_hint:
      "Buttons are disabled here — this page is a picture of a result, not the app.",

    cv_demo_score_label: "Match",
    cv_demo_matched: "Matched keywords",
    cv_demo_missing: "Missing keywords",
    cv_demo_kept: "7 of 12 experiences kept",

    cv_demo_kept_title: "Experiences the scorer kept",
    cv_demo_kept_sub:
      "Ranked by relevance. Each one is scored against the job description before it is kept — anything below the cut-off is dropped.",

    cv_demo_manual_title: "Your picks are kept",
    cv_demo_manual_sub:
      "Anything the scorer drops is still there to re-add by hand. Untick a project to hide it, or add one back, and the selection survives regenerating, reloading, and switching language.",
    cv_demo_manual_kept: "Kept by you",
    cv_demo_manual_dropped: "Dropped by you",

    cv_demo_skills_title: "Skills it surfaced",
    cv_demo_preview: "Preview & download PDF",
    cv_demo_preview_hint:
      "The real app renders the tailored CV to a clean, single-page, ATS-friendly PDF you can print straight to disk.",

    cv_demo_cta_title: "Now do it with your own CV",
    cv_demo_cta_sub:
      "The app runs entirely in your browser. Your CV never leaves your device — there is no account and no upload.",
    cv_demo_cta_tailor: "Tailor your CV →",
    cv_demo_cta_edit: "Build a CV from scratch",
    cv_demo_footer_back: "← Back to the overview"
  };

  var FR = {
    cv_nav_features: "Fonctionnalités",
    cv_nav_how: "Comment ça marche",
    cv_nav_demo: "Démo",
    cv_nav_open_app: "Ouvrir l'app",

    cv_demo_notice:
      "Voici un aperçu statique de l'application avec un CV et une offre d'emploi fictifs. Rien ici n'est réel, rien n'est enregistré, et rien ne s'exécute dans votre navigateur.",
    cv_demo_title: "Adapter un CV, de bout en bout",
    cv_demo_sub:
      "Voici à quoi ressemble une exécution : l'offre d'emploi à gauche, le score et la sélection qu'elle a produite à droite.",

    cv_demo_field_title: "Intitulé du poste",
    cv_demo_field_jd: "Offre d'emploi",
    cv_demo_field_mode: "Mode de correspondance",
    cv_demo_mode_keyword: "Mots-clés",
    cv_demo_mode_embedding: "Embedding",
    cv_demo_mode_hybrid: "Hybride",
    cv_demo_mode_hint:
      "Le mode hybride mêle la correspondance par mots-clés et la similarité sémantique, ce qui le fait gagner sur les offres qui évitent les mots exacts de votre CV.",
    cv_demo_generate: "Générer",
    cv_demo_static_hint:
      "Les boutons sont désactivés ici — cette page est une image d'un résultat, pas l'application.",

    cv_demo_score_label: "Score",
    cv_demo_matched: "Mots-clés trouvés",
    cv_demo_missing: "Mots-clés manquants",
    cv_demo_kept: "7 expériences gardées sur 12",

    cv_demo_kept_title: "Les expériences retenues par le score",
    cv_demo_kept_sub:
      "Classées par pertinence. Chacune est évaluée par rapport à l'offre avant d'être conservée — tout ce qui passe sous le seuil est écarté.",

    cv_demo_manual_title: "Vos choix sont conservés",
    cv_demo_manual_sub:
      "Tout ce que l'algorithme écarte reste là pour être réajouté à la main. Décochez un projet pour le masquer, ou remettez-le, et la sélection survit à une régénération, à un rechargement et au changement de langue.",
    cv_demo_manual_kept: "Gardé par vous",
    cv_demo_manual_dropped: "Retiré par vous",

    cv_demo_skills_title: "Les compétences remontées",
    cv_demo_preview: "Aperçu et téléchargement PDF",
    cv_demo_preview_hint:
      "La véritable application rend le CV adapté en un PDF propre, sur une seule page, lisible par les ATS, que vous pouvez imprimer directement.",

    cv_demo_cta_title: "Faites-le maintenant avec votre CV",
    cv_demo_cta_sub:
      "L'application fonctionne entièrement dans votre navigateur. Votre CV ne quitte jamais votre appareil — pas de compte, pas d'envoi.",
    cv_demo_cta_tailor: "Adapter votre CV →",
    cv_demo_cta_edit: "Créer un CV de zéro",
    cv_demo_footer_back: "← Retour à la présentation"
  };

  function currentLang() {
    var stored = null;
    try {
      stored = localStorage.getItem(LANG_KEY);
    } catch (e) {
      stored = null;
    }
    if (stored === "en" || stored === "fr") return stored;
    // No explicit choice yet: follow the browser, then default to French, which
    // is what the app itself does — so the two never disagree.
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
      // reads correctly instead of showing a raw key.
      if (text) nodes[i].textContent = text;
    }
    var langBtn = document.getElementById("langBtn");
    if (langBtn) langBtn.textContent = lang === "fr" ? "EN" : "FR";
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
      localStorage.setItem(THEME_KEY, dark ? "light" : "dark");
    } catch (e) {
      /* ignore */
    }
  };

  var lang = currentLang();
  apply(lang);
  try {
    var savedTheme = localStorage.getItem(THEME_KEY);
    if (savedTheme === "light" || savedTheme === "dark") {
      document.documentElement.setAttribute("data-theme", savedTheme);
      var themeBtn = document.getElementById("themeBtn");
      if (themeBtn) themeBtn.textContent = savedTheme === "dark" ? "☀️" : "🌙";
    }
  } catch (e) {
    /* ignore */
  }
})();