use super::*;
use crate::models::*;

// ── apply_manual_project_selection ──────────────────────────────────────

fn exp_with_projects(id: &str, company: &str, project_ids: &[&str]) -> Experience {
    Experience {
        id: id.to_string(),
        company: company.to_string(),
        projects: project_ids
            .iter()
            .map(|pid| ExperienceProject {
                id: pid.to_string(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn apply_manual_selection_keeps_only_checked_projects() {
    let cv = LifetimeCV {
        experiences: vec![exp_with_projects("e1", "Acme", &["p1", "p2", "p3"])],
        ..Default::default()
    };
    let checked: HashSet<String> = ["p1", "p3"].iter().map(|s| s.to_string()).collect();
    let result = apply_manual_project_selection(&cv, &checked);
    assert_eq!(result.len(), 1);
    let ids: Vec<&str> = result[0].projects.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["p1", "p3"]);
}

#[test]
fn apply_manual_selection_drops_experience_with_zero_checked_projects() {
    let cv = LifetimeCV {
        experiences: vec![exp_with_projects("e1", "Acme", &["p1", "p2"])],
        ..Default::default()
    };
    // Neither p1 nor p2 is checked — the whole experience should
    // disappear, not appear with an empty projects list.
    let checked: HashSet<String> = HashSet::new();
    let result = apply_manual_project_selection(&cv, &checked);
    assert!(result.is_empty());
}

#[test]
fn apply_manual_selection_can_reinclude_an_experience_the_algorithm_excluded() {
    // The whole point of the feature: a project id can be checked even
    // if the automatic pass never selected that experience at all —
    // reading from `cv` (not an already-filtered TailoredCV) is what
    // makes this possible.
    let cv = LifetimeCV {
        experiences: vec![
            exp_with_projects("e1", "Kept", &["p1"]),
            exp_with_projects("e2", "ManuallyReincluded", &["p2"]),
        ],
        ..Default::default()
    };
    let checked: HashSet<String> = ["p1", "p2"].iter().map(|s| s.to_string()).collect();
    let result = apply_manual_project_selection(&cv, &checked);
    assert_eq!(result.len(), 2);
    assert_eq!(result[1].company, "ManuallyReincluded");
}

// Regression test for a real bug: an earlier version of this function
// took an explicit `experience_order` parameter seeded from
// `debug_scores` (score-sorted order). Applying a manual selection
// then silently reordered the whole CV to relevance order — e.g. the
// most recent role (which should stay at the top, reverse-
// chronological, matching the automatic path's own output) got
// shoved down to wherever it happened to rank by raw keyword score.
// The fix: always follow `cv.experiences`' own stored order, exactly
// like the automatic path's "Rebuild the selection in the CV's
// original order" step does — never an externally-supplied order.
#[test]
fn apply_manual_selection_always_follows_cv_storage_order_not_an_external_order() {
    let cv = LifetimeCV {
        experiences: vec![
            exp_with_projects("e1", "First", &["p1"]),
            exp_with_projects("e2", "Second", &["p2"]),
        ],
        ..Default::default()
    };
    let checked: HashSet<String> = ["p1", "p2"].iter().map(|s| s.to_string()).collect();
    let result = apply_manual_project_selection(&cv, &checked);
    assert_eq!(
        result[0].company, "First",
        "output order must match cv.experiences' own order"
    );
    assert_eq!(result[1].company, "Second");
}

// ── expand_summary_skills ────────────────────────────────────────────────

fn skills_named(names: &[&str]) -> Vec<Skill> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| Skill {
            id: format!("s{i}"),
            name: n.to_string(),
            ..Default::default()
        })
        .collect()
}

#[test]
fn expand_summary_skills_replaces_placeholder_with_top_cap_skills() {
    let skills = skills_named(&[
        "Rust",
        "Kubernetes",
        "Docker",
        "PostgreSQL",
        "Terraform",
        "AWS",
    ]);
    let out = expand_summary_skills(
        "Distributed systems engineer focused on {{skills}}.",
        &skills,
        SUMMARY_SKILLS_CAP,
    );
    assert_eq!(out, "Distributed systems engineer focused on Rust, Kubernetes, Docker, PostgreSQL, Terraform.");
}

#[test]
fn expand_summary_skills_caps_and_handles_unrelated_skill_position() {
    let skills = skills_named(&["Rust", "Docker"]);
    let out = expand_summary_skills(
        "Deep in {{skills}}; fan of Bash.",
        &skills,
        SUMMARY_SKILLS_CAP,
    );
    assert_eq!(out, "Deep in Rust, Docker; fan of Bash.");
}

#[test]
fn expand_summary_skills_is_noop_without_placeholder_or_without_skills() {
    assert_eq!(
        expand_summary_skills("No placeholder here.", &skills_named(&["Rust"]), 5),
        "No placeholder here."
    );
    assert_eq!(
        expand_summary_skills(
            "Uses {{skills}} but none survived.",
            &[],
            SUMMARY_SKILLS_CAP
        ),
        "Uses {{skills}} but none survived.",
        "an empty skill list must keep the placeholder literal, not vanish it"
    );
}

#[test]
fn resolve_summary_falls_back_to_default_when_not_chosen_or_unknown() {
    let personal = crate::models::PersonalInfo {
        summary: crate::models::LocalizedText::same("Engineer biography"),
        summaries: vec![crate::models::NamedSummary {
            name: "Leadership".to_string(),
            text: crate::models::LocalizedText::same("People-focused bio"),
        }],
        ..Default::default()
    };
    let skills = skills_named(&["Rust"]);
    assert_eq!(
        resolve_summary(&personal, None, &skills).en,
        "Engineer biography"
    );
    assert_eq!(
        resolve_summary(&personal, Some("DoesNotExist"), &skills).en,
        "Engineer biography",
        "a name that matches no variant must fall back to the default"
    );
}

#[test]
fn resolve_summary_picks_variant_and_expands_skills_in_both_languages() {
    let personal = crate::models::PersonalInfo {
        summary: crate::models::LocalizedText::same("Engineer biography"),
        summaries: vec![crate::models::NamedSummary {
            name: "Platform".to_string(),
            text: crate::models::LocalizedText {
                en: "Platform bio over {{skills}}.".to_string(),
                fr: "Bio plateforme sur {{skills}}.".to_string(),
            },
        }],
        ..Default::default()
    };
    let skills = skills_named(&["Rust", "Docker"]);
    let out = resolve_summary(&personal, Some("Platform"), &skills);
    assert_eq!(out.en, "Platform bio over Rust, Docker.");
    assert_eq!(out.fr, "Bio plateforme sur Rust, Docker.");
}

// A CV where "Ansible" sits last: if ordering were CV-list-order the
// Ansible pick would be pushed out of the automatic top-5 by the four
// skills that precede it, even though the JD is entirely about Ansible.
fn relevance_fixture() -> LifetimeCV {
    LifetimeCV {
        skills: vec![
            Skill {
                id: "s-web".to_string(),
                name: "Web Development".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-ui".to_string(),
                name: "UI Design".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-testing".to_string(),
                name: "Testing".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-sql".to_string(),
                name: "SQL".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-ansible".to_string(),
                name: "Ansible".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-cooking".to_string(),
                name: "Baking".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn sort_skills_by_relevance_orders_by_jd_match_not_cv_position() {
    let cv = relevance_fixture();
    let jd = "Looking for an Ansible automation expert. Ansible playbooks, Ansible roles, Ansible inventory. Ansible.";
    let mut sorted = sort_skills_by_relevance(&cv, &cv.skills, jd);
    assert_eq!(sorted.first().map(|s| s.id.as_str()), Some("s-ansible"));
    assert!(
        !sorted.iter().any(|s| s.name == "Baking"),
        "unrelated skill must be dropped, not just sorted to the back"
    );
    sorted.truncate(SUMMARY_SKILLS_CAP);
    assert!(
        sorted.iter().any(|s| s.id == "s-ansible"),
        "Ansible must survive the top-{} slice",
        SUMMARY_SKILLS_CAP
    );
}

#[test]
fn summary_skills_for_uses_override_ids_when_provided() {
    let cv = relevance_fixture();
    let tailored_skills = sort_skills_by_relevance(&cv, &cv.skills, "Ansible Deploy");
    let overridden = summary_skills_for(
        &cv,
        &tailored_skills,
        "Ansible Deploy",
        &["s-sql".to_string(), "s-ui".to_string()],
    );
    assert_eq!(
        overridden.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["s-ui", "s-sql"],
        "override must win and follow CV order, not the tailored order"
    );
    let auto = summary_skills_for(&cv, &tailored_skills, "Ansible Deploy", &[]);
    assert!(
        auto.iter().any(|s| s.id == "s-ansible"),
        "empty override must fall back to the automatic relevance order"
    );
    // Hand-picking is broader than the tailored list: "Baking" is never
    // JD-related (dropped by the automatic filter), yet a manual override
    // must still resolve it for the {{skills}} placeholder — the picker
    // lists every CV skill, not just the algorithm's survivors.
    let hand_picked = summary_skills_for(
        &cv,
        &tailored_skills,
        "Ansible Deploy",
        &["s-cooking".to_string()],
    );
    assert_eq!(
        hand_picked
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        vec!["s-cooking"],
        "override must be able to pull in a skill the algorithm dropped"
    );
}

#[test]
fn apply_manual_skill_selection_keeps_only_checked_skills_in_cv_order() {
    let cv = LifetimeCV {
        skills: vec![
            Skill {
                id: "s1".into(),
                name: "Rust".into(),
                ..Default::default()
            },
            Skill {
                id: "s2".into(),
                name: "Docker".into(),
                ..Default::default()
            },
            Skill {
                id: "s3".into(),
                name: "Bash".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    // Checked out of order + a missing one — output must follow the CV's
    // own skill order, not the checkbox iteration order.
    let checked: HashSet<String> = ["s3", "s1"].iter().map(|s| s.to_string()).collect();
    let result = apply_manual_skill_selection(&cv, &checked);
    let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Rust", "Bash"]);
}

#[test]
fn apply_manual_skill_selection_empty_set_drops_everything() {
    let cv = LifetimeCV {
        skills: vec![Skill {
            id: "s1".into(),
            name: "Rust".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let result = apply_manual_skill_selection(&cv, &HashSet::new());
    assert!(result.is_empty());
}

#[test]
fn apply_manual_skill_selection_can_reinclude_a_skill_the_algorithm_dropped() {
    // Like the project counterpart: reading from `cv` (not the already
    // filtered TailoredCV) is what lets a person re-add an unrelated
    // non-Expert skill that the automatic pass excluded.
    let cv = LifetimeCV {
        skills: vec![
            Skill {
                id: "s-rust".into(),
                name: "Rust".into(),
                ..Default::default()
            },
            Skill {
                id: "s-py".into(),
                name: "Python".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let checked: HashSet<String> = ["s-rust", "s-py"].iter().map(|s| s.to_string()).collect();
    let result = apply_manual_skill_selection(&cv, &checked);
    let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Rust", "Python"]);
}

// ── Fixture ───────────────────────────────────────────────────────────────

fn fixture_cv() -> LifetimeCV {
    LifetimeCV {
        personal: PersonalInfo {
            name: "Jane Smith".to_string(),
            title: LocalizedText::same("Backend Engineer"),
            summary: LocalizedText::same("Experienced distributed-systems developer"),
            ..Default::default()
        },
        experiences: vec![
            Experience {
                id: "exp-1".to_string(),
                company: "Acme Corp".to_string(),
                role: LocalizedText::same("Software Engineer"),
                start_date: "Jan 2021".to_string(),
                end_date: "Present".to_string(),
                projects: vec![ExperienceProject {
                    name: LocalizedText::same("Distributed Systems"),
                    context: vec![LocalizedText::same(
                        "High-throughput microservices architecture",
                    )],
                    bullets: vec![
                        LocalizedText::same("Built distributed systems using Rust and Tokio"),
                        LocalizedText::same("Reduced API latency by 40% through caching"),
                    ],
                    skill_ids: vec!["s1".to_string(), "s2".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "exp-2".to_string(),
                company: "Beta Ltd".to_string(),
                role: LocalizedText::same("Junior Developer"),
                start_date: "Jun 2019".to_string(),
                end_date: "Dec 2020".to_string(),
                projects: vec![ExperienceProject {
                    name: LocalizedText::same("Web Applications"),
                    context: vec![LocalizedText::same("Customer-facing portal overhaul")],
                    bullets: vec![LocalizedText::same(
                        "Developed web applications with React and TypeScript",
                    )],
                    skill_ids: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        skills: vec![
            Skill {
                id: "s1".to_string(),
                name: "Rust".to_string(),
                category: SkillCategory::Programming,
                level: SkillLevel::Expert,
            },
            Skill {
                id: "s2".to_string(),
                name: "PostgreSQL".to_string(),
                category: SkillCategory::Database,
                level: SkillLevel::Advanced,
            },
            Skill {
                id: "s3".to_string(),
                name: "Python".to_string(),
                category: SkillCategory::Programming,
                level: SkillLevel::Intermediate,
            },
        ],
        projects: vec![Project {
            id: "p1".to_string(),
            name: "cv-generator".to_string(),
            description: LocalizedText::same("CV generator written in Rust using Dioxus"),
            tools: vec!["Rust".to_string(), "Dioxus".to_string()],
            bullets: vec![LocalizedText::same("Keyword matching algorithm")],
            ..Default::default()
        }],
        ..Default::default()
    }
}

// ── tokenise / extract_keywords ───────────────────────────────────────────

#[test]
fn keywords_basic_extraction() {
    let kws = extract_keywords("Rust developer with PostgreSQL experience");
    let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();
    assert!(names.contains(&"rust"));
    assert!(names.contains(&"developer"));
    assert!(names.contains(&"postgresql"));
}

#[test]
fn keywords_stop_words_are_removed() {
    // Every word here is a stop word
    let kws = extract_keywords("the and for in with of to a is are was were");
    assert!(
        kws.is_empty(),
        "All words are stop words — expected empty, got {:?}",
        kws
    );
}

#[test]
fn keywords_short_words_are_removed() {
    // "go", "ai", "ml" are all < 3 chars
    let kws = extract_keywords("go ai ml");
    for (k, _) in &kws {
        assert!(k.len() >= 3, "Keyword '{}' is shorter than 3 chars", k);
    }
}

#[test]
fn keywords_sorted_by_frequency_descending() {
    let kws = extract_keywords("rust rust rust python python java");
    assert!(!kws.is_empty());
    assert_eq!(kws[0].0, "rust", "rust (×3) should rank first");
    assert_eq!(kws[0].1, 3);
    assert_eq!(kws[1].0, "python", "python (×2) should rank second");
    assert_eq!(kws[1].1, 2);
}

#[test]
fn keywords_case_insensitive() {
    // extract_keywords now also emits bigrams/trigrams (e.g. "rust rust"),
    // so the three case variants no longer collapse to a single overall
    // entry — but they must still collapse to a single *unigram* entry,
    // which should rank first since it has the highest frequency.
    let kws = extract_keywords("Rust RUST rust");
    assert_eq!(kws[0].0, "rust");
    assert_eq!(kws[0].1, 3);
    let unigram_entries = kws.iter().filter(|(k, _)| !k.contains(' ')).count();
    assert_eq!(
        unigram_entries, 1,
        "All case variants should collapse to a single unigram entry"
    );
}

// ── select_by_relative_cutoff ────────────────────────────────────────────

#[test]
fn cutoff_keyword_like_spread_keeps_only_top_without_mean_floor() {
    // Wide spread, e.g. real keyword scores: only the top one clears 0.7*max.
    let scores = vec![0.82, 0.30, 0.10, 0.05];
    let kept = select_by_relative_cutoff(&scores, 0.7, false);
    assert_eq!(kept, vec![0]);
}

#[test]
fn cutoff_keyword_like_near_ties_all_kept_without_mean_floor() {
    // 4 genuinely-similar, all-relevant scores (the case the old
    // project-level code was tuned to preserve) — fixed fraction alone
    // keeps all 4, exactly like before this change.
    let scores = vec![0.82, 0.79, 0.75, 0.71];
    let mut kept = select_by_relative_cutoff(&scores, 0.7, false);
    kept.sort();
    assert_eq!(kept, vec![0, 1, 2, 3]);
}

#[test]
fn cutoff_embedding_like_tight_cluster_trimmed_with_mean_floor() {
    // Tightly clustered cosine-similarity-like scores (everything
    // sits within 0.7x of the max, so the fixed fraction alone can't
    // discriminate at all — this is the exact "6 of 7 experiences
    // included instead of ~5" symptom this fix addresses).
    let scores = vec![0.75, 0.72, 0.70, 0.68, 0.65, 0.60, 0.58];
    let fixed_only = select_by_relative_cutoff(&scores, 0.7, false);
    let with_mean_floor = select_by_relative_cutoff(&scores, 0.7, true);
    assert_eq!(
        fixed_only.len(),
        scores.len(),
        "fixed fraction alone should barely filter a tight cluster"
    );
    assert!(
        with_mean_floor.len() < fixed_only.len(),
        "mean floor should trim the tight cluster down, got {:?}",
        with_mean_floor
    );
    let mut with_mean_floor_sorted = with_mean_floor.clone();
    with_mean_floor_sorted.sort();
    // Index 4 (0.65) is included too: it's within the 3% near-tie
    // margin below the mean-floor cutoff (see NEAR_TIE_MARGIN_FRACTION).
    assert_eq!(with_mean_floor_sorted, vec![0, 1, 2, 3, 4]);
}

#[test]
fn cutoff_near_tie_margin_rescues_a_narrow_miss_but_not_a_real_gap() {
    // These are the *actual* observed scores from a real embedding run
    // (DTNUM, SIRIUS, proxIT, KAIMAN, BRED IT, CA-GIP, Groupe HN) that
    // motivated this margin: KAIMAN (genuinely the most relevant
    // remaining experience for an AWX/CIS-hardening JD) missed the
    // mean-floor cutoff by 0.0094 while proxIT (pure mainframe, no
    // AWX/Ansible/CIS content at all) cleared it — out of an overall
    // score range of about 0.18. That gap is noise, not signal, and
    // dropping KAIMAN over it was the wrong call.
    let scores = vec![0.4858, 0.4784, 0.4070, 0.3954, 0.3866, 0.3755, 0.3047];
    let kept = select_by_relative_cutoff(&scores, 0.5, true);
    let mut kept_sorted = kept.clone();
    kept_sorted.sort();
    // indices: 0=DTNUM, 1=SIRIUS, 2=proxIT, 3=KAIMAN kept;
    // 4=BRED IT, 5=CA-GIP, 6=Groupe HN correctly still excluded —
    // the margin rescues a narrow miss, it doesn't just keep everyone.
    assert_eq!(kept_sorted, vec![0, 1, 2, 3]);
}

#[test]
fn cutoff_empty_scores_returns_empty() {
    assert!(select_by_relative_cutoff(&[], 0.7, false).is_empty());
    assert!(select_by_relative_cutoff(&[], 0.7, true).is_empty());
}

#[test]
fn cutoff_zero_scores_excluded_even_under_cutoff() {
    // A 0.0 score should never be selected regardless of where the
    // cutoff lands (mirrors the old `*s > 0.0` guard).
    let scores = vec![0.0, 0.0, 0.0];
    assert!(select_by_relative_cutoff(&scores, 0.7, false).is_empty());
    assert!(select_by_relative_cutoff(&scores, 0.7, true).is_empty());
}

// Regression coverage for a real data-modeling gap: a CV author (or
// PDF import) can write one combined tools line covering all of a
// role's sub-projects, leaving individual `ExperienceProject.skill_ids`
// fields empty except on whichever project happened to end up holding
// it. `pooled_tools` is what lets per-project scoring still credit a
// project for a tool that's only recorded on a sibling within the same
// experience — see its doc comment for the full rationale.
#[test]
fn pooled_tools_unions_across_sibling_projects_without_duplicates() {
    let skills = vec![
        Skill {
            id: "s-ansible".to_string(),
            name: "Ansible".to_string(),
            ..Default::default()
        },
        Skill {
            id: "s-awx".to_string(),
            name: "AWX".to_string(),
            ..Default::default()
        },
        Skill {
            id: "s-terraform".to_string(),
            name: "Terraform".to_string(),
            ..Default::default()
        },
    ];
    let p1 = ExperienceProject {
        skill_ids: vec!["s-ansible".to_string(), "s-awx".to_string()],
        ..Default::default()
    };
    let p2 = ExperienceProject {
        // "s-awx" repeated on purpose: must not appear twice in the pool.
        skill_ids: vec!["s-awx".to_string(), "s-terraform".to_string()],
        ..Default::default()
    };
    let p3 = ExperienceProject {
        skill_ids: vec![],
        ..Default::default()
    };
    let pooled = pooled_tools(&[p1, p2, p3], &skills);
    assert_eq!(pooled, vec!["Ansible", "AWX", "Terraform"]);
}

#[test]
fn experience_project_scoring_credits_tools_only_recorded_on_a_sibling() {
    // Mirrors the real KAIMAN scenario: the project whose own bullets
    // actually reference AWX has NO skill tags at all, while a sibling
    // project (unrelated bullets) holds the tags for the whole role.
    // Without pooling, the AWX-mentioning project would get no credit
    // at all for the "awx"/"ansible" keywords beyond whatever it
    // happens to say in prose.
    let keywords = vec![("awx".to_string(), 5), ("ansible".to_string(), 5)];
    let skills = vec![
        Skill {
            id: "s-awx".to_string(),
            name: "AWX".to_string(),
            ..Default::default()
        },
        Skill {
            id: "s-ansible".to_string(),
            name: "Ansible".to_string(),
            ..Default::default()
        },
    ];

    let mentions_awx_no_tags = ExperienceProject {
        bullets: vec![crate::models::LocalizedText::same(
            "conçu des playbooks awx pour ce projet",
        )],
        skill_ids: vec![],
        ..Default::default()
    };
    let sibling_holds_the_tags = ExperienceProject {
        bullets: vec![crate::models::LocalizedText::same(
            "support technique sans rapport",
        )],
        skill_ids: vec!["s-awx".to_string(), "s-ansible".to_string()],
        ..Default::default()
    };

    // A tiny synthetic IDF corpus is enough here — we're only checking
    // that pooling changes the *relative* score, not testing IDF
    // weighting itself.
    let idf = Idf::build(&[
        vec!["awx".to_string(), "ansible".to_string()],
        vec!["unrelated".to_string(), "terms".to_string()],
    ]);

    let shared_tools = pooled_tools(
        &[mentions_awx_no_tags.clone(), sibling_holds_the_tags.clone()],
        &skills,
    );

    let score_without_pooling =
        score_experience_project(&mentions_awx_no_tags, &keywords, &idf, &[]);
    let score_with_pooling =
        score_experience_project(&mentions_awx_no_tags, &keywords, &idf, &shared_tools);

    assert!(
        score_with_pooling > score_without_pooling,
        "pooling sibling tools should raise this project's score \
         (without: {score_without_pooling}, with: {score_with_pooling})"
    );
}

#[test]
fn keywords_preserves_plus_in_token() {
    // C++ is 3 chars after lowercasing — it passes the len >= 3 guard.
    // C# is only 2 chars and is *correctly* filtered; this is expected behaviour
    // (the min-length guard intentionally drops very short tokens to reduce noise).
    let kws = extract_keywords("C++ developer and C# engineer");
    let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();

    assert!(
        names.contains(&"c++"),
        "c++ (3 chars) should survive the length filter"
    );
    assert!(
        names.contains(&"developer"),
        "common words should be extracted"
    );
    assert!(
        names.contains(&"engineer"),
        "common words should be extracted"
    );
    assert!(
        !names.contains(&"c#"),
        "c# (2 chars) is correctly filtered by len >= 3"
    );
    assert!(!names.contains(&"and"), "stop word 'and' should be removed");
}

#[test]
fn keywords_empty_input_returns_empty() {
    assert!(extract_keywords("").is_empty());
    assert!(extract_keywords("   ").is_empty());
}

// ── score_text ────────────────────────────────────────────────────────────

// An empty-corpus Idf falls back to a default weight of 1.0 for every
// term (see `Idf::get`), which makes these tests equivalent to plain
// frequency weighting — the same behaviour the old (pre-TF-IDF) tests
// asserted on.
fn no_idf() -> Idf {
    Idf::build(&[])
}

#[test]
fn score_text_perfect_match_is_one() {
    let kws = vec![("rust".to_string(), 2), ("postgresql".to_string(), 1)];
    let s = score_text("rust postgresql developer", &kws, &no_idf());
    assert_eq!(s, 1.0);
}

#[test]
fn score_text_no_match_is_zero() {
    let kws = vec![("golang".to_string(), 1), ("java".to_string(), 1)];
    let s = score_text("rust postgresql developer", &kws, &no_idf());
    assert_eq!(s, 0.0);
}

#[test]
fn score_text_partial_match_weighted() {
    // rust weight=2, java weight=1, total=3; only rust matches → 2/3
    let kws = vec![("rust".to_string(), 2), ("java".to_string(), 1)];
    let s = score_text("senior rust developer", &kws, &no_idf());
    let expected = 2.0_f32 / 3.0_f32;
    assert!(
        (s - expected).abs() < 1e-4,
        "Expected {:.4}, got {:.4}",
        expected,
        s
    );
}

#[test]
fn score_text_empty_inputs_return_zero() {
    assert_eq!(score_text("", &[], &no_idf()), 0.0);
    assert_eq!(score_text("rust", &[], &no_idf()), 0.0);
    assert_eq!(score_text("", &[("rust".to_string(), 1)], &no_idf()), 0.0);
}

#[test]
fn score_text_is_case_insensitive() {
    let kws = vec![("rust".to_string(), 1)];
    // keyword is lowercase; text has uppercase — should still match
    assert_eq!(score_text("RUST Engineer", &kws, &no_idf()), 1.0);
}

// ── Idf exact values ──────────────────────────────────────────────────────

fn three_doc_idf() -> Idf {
    Idf::build(&[
        vec!["rust".into(), "wasm".into()],
        vec!["python".into(), "wasm".into()],
        vec!["rust".into(), "linux".into()],
    ])
}

#[test]
fn idf_correct_smoothed_weights() {
    // n = 3 docs. idf = ln((n+1)/(df+1)) + 1.
    // rust: df=2 → ln(4/3)+1; linux: df=1 → ln(4/2)+1 = ln(2)+1.
    let idf = three_doc_idf();
    let rust_expected = (4.0_f32 / 3.0).ln() + 1.0;
    let linux_expected = (4.0_f32 / 2.0).ln() + 1.0;
    assert!(
        (idf.get("rust") - rust_expected).abs() < 1e-5,
        "rust idf wrong"
    );
    assert!(
        (idf.get("linux") - linux_expected).abs() < 1e-5,
        "linux idf wrong"
    );
    // get() must NOT always return the default 1.0 for a known term.
    assert!(
        (idf.get("rust") - 1.0).abs() > 1e-5,
        "known term got default weight"
    );
}

#[test]
fn idf_adds_doc_frequencies_across_unique_terms() {
    // Exercising different df values makes the `+=` (df increment) and the
    // idf formula operator mutations observable: with only single-doc terms
    // every idf would be identical and those mutants couldn't be told apart.
    let idf = three_doc_idf();
    // "rust" appears in 2 docs (df=2), "linux" in 1 (df=1) — different idf.
    assert!(
        (idf.get("rust") - idf.get("linux")).abs() > 1e-4,
        "different df must produce different idf: {:?} vs {:?}",
        idf.get("rust"),
        idf.get("linux")
    );
}

#[test]
fn score_text_partial_with_weighted_idf() {
    // With real (non-unit) idf, the freq*idf products and the matched/total
    // ratio are exact fractions — this discriminates the *→/ operator
    // mutations in both total and matched weights (they collapse to the
    // same value only when idf == 1.0, which no_idf() would hide).
    let idf = three_doc_idf();
    let keywords = vec![
        ("rust".to_string(), 2),
        ("wasm".to_string(), 1),
        ("linux".to_string(), 3),
    ];
    let score = score_text("rust wasm", &keywords, &idf);
    let a = (4.0_f32 / 3.0).ln() + 1.0; // rust & wasm idf
    let b = (4.0_f32 / 2.0).ln() + 1.0; // linux idf
    let matched = 2.0 * a + 1.0 * a;
    let total = 2.0 * a + 1.0 * a + 3.0 * b;
    let expected = matched / total;
    assert!(
        (score - expected).abs() < 1e-5,
        "expected ~{expected}, got {score}"
    );
}

// ── fuzzy_eq edge cases ───────────────────────────────────────────────────

#[test]
fn fuzzy_eq_multiword_never_fuzzy_matches() {
    // Any whitespace in either operand means "multi-word phrase" → must
    // NEVER fuzzy match (fuzzy only ever applies to single words). This
    // catches the ||→&& mutation which would wrongly allow fuzziness.
    assert!(!fuzzy_eq("hardning", "hardening "));
    assert!(!fuzzy_eq("hardening", "hardning "));
    assert!(!fuzzy_eq("two words", "hardening"));
}

#[test]
fn fuzzy_eq_long_word_tolerates_distance_two() {
    // max_len >= 8 grants tolerance 2, so a 2-edit typo is allowed. This
    // pins the max_len >= 8 boundary (a >=→< mutation drops this case).
    assert!(fuzzy_eq("aaaaaaaa", "aaaaaa"));
}

#[test]
fn fuzzy_eq_five_char_word_tolerates_single_edit() {
    // max_len >= 5 (but < 8) grants tolerance 1. A 1-edit typo is allowed
    // for a 5-char word (>=→< here would wrongly disallow it).
    assert!(fuzzy_eq("aaaaa", "aaaa"));
    assert!(!fuzzy_eq("abc", "abd")); // < 5 chars → no fuzziness, exact only
}

// ── score_experience / score_skill / score_project / display ─────────────

#[test]
fn score_experience_resolves_skill_into_text_for_scoring() {
    // The experience's text is built from role/company/projects AND the
    // resolved skill names. Here the ONLY match comes from the skill name
    // itself, so scoring depends on the ==→!= id filter and the function
    // must return a strict fraction (never a flat 0/1/-1).
    let skills = vec![Skill {
        id: "s1".to_string(),
        name: "rust".to_string(),
        ..Default::default()
    }];
    let exp = Experience {
        projects: vec![ExperienceProject {
            skill_ids: vec!["s1".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let keywords = vec![("rust".to_string(), 1), ("zz".to_string(), 1)];
    let score = score_experience(&exp, &keywords, &no_idf(), &skills);
    let expected = 0.5; // only "rust" matches out of two keywords
    assert!(
        (score - expected).abs() < 1e-5,
        "expected {expected}, got {score} — must be a strict fraction"
    );
}

#[test]
fn score_skill_scored_by_name() {
    let skill = Skill {
        name: "rust".to_string(),
        ..Default::default()
    };
    let keywords = vec![("rust".to_string(), 1), ("zz".to_string(), 1)];
    let score = score_skill(&skill, &keywords, &no_idf());
    assert!((score - 0.5).abs() < 1e-5, "expected 0.5, got {score}");
}

#[test]
fn score_project_builds_text_from_project_fields() {
    let proj = Project {
        name: "rust".to_string(),
        ..Default::default()
    };
    let keywords = vec![("rust".to_string(), 1), ("zz".to_string(), 1)];
    let score = score_project(&proj, &keywords, &no_idf());
    // project_text includes the name (and empty desc/tools/bullets), so
    // "rust" matches exactly one of the two keywords.
    assert!((score - 0.5).abs() < 1e-5, "expected 0.5, got {score}");
}

#[test]
fn display_prefers_french_when_present() {
    use crate::models::LocalizedText;
    let role = LocalizedText {
        en: "Engineer".into(),
        fr: "Ingénieur".into(),
    };
    assert_eq!(display_role(&role), "Ingénieur");
    let name = LocalizedText {
        en: "Project".into(),
        fr: "Projet".into(),
    };
    assert_eq!(display_name(&name), "Projet");
}

#[test]
fn display_falls_back_to_english_when_french_absent() {
    use crate::models::LocalizedText;
    let role = LocalizedText {
        en: "Engineer".into(),
        fr: String::new(),
    };
    assert_eq!(display_role(&role), "Engineer");
    let name = LocalizedText {
        en: "Project".into(),
        fr: String::new(),
    };
    assert_eq!(display_name(&name), "Project");
}

// ── stemming / synonyms / fuzzy matching ────────────────────────────────

#[test]
fn synonyms_collapse_fr_en_variants() {
    // "hardening" (EN) and "durcissement" (FR) should normalize to the
    // same canonical term via the synonym dictionary.
    assert_eq!(normalize("hardening"), normalize("durcissement"));
    assert_eq!(normalize("hardening"), normalize("sécurisation"));
}

#[test]
fn stemming_collapses_inflections() {
    // French plural/verb-form variants of "déploiement" should share a
    // stem, and so should the English "deploy"/"deployment" family.
    assert_eq!(stem("deploiement"), stem("deploiements"));
    assert_eq!(normalize("deploiement"), normalize("deploying"));
}

#[test]
fn fuzzy_eq_tolerates_small_typos_not_big_ones() {
    assert!(fuzzy_eq("hardening", "hardning")); // dropped letter, len>=8 → tolerance 2
    assert!(!fuzzy_eq("cis", "sql")); // short words, no fuzziness allowed
    assert!(!fuzzy_eq("hardening", "monitoring")); // unrelated words, too far apart
}

#[test]
fn fuzzy_eq_eight_char_boundary_tolerance_is_exactly_two() {
    // Pins the len>=8 → tolerance 2 boundary. An 8-char word at exactly
    // distance 2 must pass; at distance 3 must fail. If the `>=8`
    // comparison were off-by-one (say it only granted tolerance up to 7
    // chars, or the tolerance were 1), these flip.
    assert!(fuzzy_eq("abcdefgh", "abcdefXY")); // distance 2 → allowed
    assert!(!fuzzy_eq("abcdefgh", "abcdeXYZ")); // distance 3 → too far
}

#[test]
fn score_text_counts_a_keyword_matched_fuzzily() {
    // Pins that score_text matches terms through terms_contain, i.e. the
    // fuzzy_eq path, not just exact token equality. Here the keyword
    // "hardning" (typo of "hardening") never appears in the text, but
    // "hardening" is a len>=8 word within the fuzzy tolerance, so the
    // keyword should still be credited — returning the same score as an
    // exact match of the typo would.
    let exact = score_text(
        "hardening measures",
        &[("hardning".to_string(), 1)],
        &no_idf(),
    );
    let unrelated = score_text(
        "hardening measures",
        &[("zzzzzzz".to_string(), 1)], // 7 chars, no typo relationship
        &no_idf(),
    );
    assert_eq!(exact, 1.0, "fuzzy-matching keyword should count as matched");
    assert_eq!(unrelated, 0.0, "unrelated keyword should not fuzzy-match");
}

#[test]
fn ngrams_capture_multiword_phrases() {
    let kws = extract_keywords("gestion de version rollback");
    let names: Vec<&str> = kws.iter().map(|(k, _)| k.as_str()).collect();
    // "de" is a stop word, so the surviving bigram is "gestion version".
    assert!(
        names.iter().any(|n| n.contains(' ')),
        "expected at least one multi-word term, got {:?}",
        names
    );
}

// ── tailor_cv ─────────────────────────────────────────────────────────────

#[test]
fn tailor_drops_unrelated_non_expert_skills() {
    // fixture_cv: Rust = Expert, PostgreSQL = Advanced, Python =
    // Intermediate. The JD only relates to Rust + PostgreSQL, so Python
    // (unrelated AND not Expert/Mastery) must be dropped from the
    // tailored skills entirely; Rust/PostgreSQL stay, Rust first.
    let cv = fixture_cv();
    let jd = "We need a Rust developer with PostgreSQL knowledge for backend systems";
    let result = tailor_cv(&cv, jd);

    let names: Vec<&str> = result
        .tailored
        .skills
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    let rust_pos = names
        .iter()
        .position(|&n| n == "Rust")
        .expect("Rust should be in skills");
    let pg_pos = names
        .iter()
        .position(|&n| n == "PostgreSQL")
        .expect("PostgreSQL should be in skills");
    assert!(
        rust_pos < pg_pos,
        "Rust should rank before PostgreSQL for a Rust-focused JD"
    );
    assert!(
        !names.contains(&"Python"),
        "unrelated, non-Expert Python must be dropped, got {names:?}"
    );
}

#[test]
fn tailor_keeps_expert_and_related_skills_only() {
    let cv = LifetimeCV {
        skills: vec![
            Skill {
                id: "s-java".into(),
                name: "Java".into(),
                level: SkillLevel::Expert,
                ..Default::default()
            },
            Skill {
                id: "s-k8s".into(),
                name: "Kubernetes".into(),
                level: SkillLevel::Mastery,
                ..Default::default()
            },
            Skill {
                id: "s-dkr".into(),
                name: "Docker".into(),
                level: SkillLevel::Intermediate,
                ..Default::default()
            },
            Skill {
                id: "s-bash".into(),
                name: "Bash".into(),
                level: SkillLevel::Advanced,
                ..Default::default()
            },
            Skill {
                id: "s-py".into(),
                name: "Python".into(),
                level: SkillLevel::Beginner,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    // JD only relates to Docker — Java/Kubernetes survive by being
    // expert-tier, Bash/Python (neither related nor expert-tier) drop.
    let result = tailor_cv(&cv, "Docker container orchestration");
    let names: Vec<&str> = result
        .tailored
        .skills
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Docker", "Java", "Kubernetes"],
        "related first, then expert-tier in original order, everything else dropped"
    );
}

#[test]
fn tailor_scorer_keeps_expert_unrelated_skill() {
    // Hybrid-mode parity: an unrelated skill survives if it is
    // self-assessed Expert, even though it scores 0 against the JD.
    let cv = LifetimeCV {
        skills: vec![
            Skill {
                id: "s-rust".into(),
                name: "rust engineer".into(),
                level: SkillLevel::Intermediate,
                ..Default::default()
            },
            Skill {
                id: "s-acct".into(),
                name: "accounting".into(),
                level: SkillLevel::Expert,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    let names: Vec<&str> = result
        .tailored
        .skills
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["rust engineer", "accounting"]);
}

#[test]
fn tailor_always_includes_at_least_two_experiences() {
    let cv = fixture_cv();
    // Completely unrelated JD — nothing in CV should score
    let jd = "Certified accountant required for tax returns and bookkeeping";
    let result = tailor_cv(&cv, jd);
    assert!(
        result.tailored.experiences.len() >= 2,
        "Should always keep ≥ 2 experiences, got {}",
        result.tailored.experiences.len()
    );
}

#[test]
fn tailor_match_score_in_valid_range() {
    let cv = fixture_cv();

    let good_jd = "Rust engineer PostgreSQL Kubernetes distributed systems async";
    let r_good = tailor_cv(&cv, good_jd);
    assert!(r_good.tailored.match_score >= 0.0 && r_good.tailored.match_score <= 1.0);

    let bad_jd = "Accountant needed for spreadsheet tax financial reporting";
    let r_bad = tailor_cv(&cv, bad_jd);
    assert!(r_bad.tailored.match_score >= 0.0 && r_bad.tailored.match_score <= 1.0);

    assert!(
        r_good.tailored.match_score > r_bad.tailored.match_score,
        "Relevant JD should score higher than unrelated one"
    );
}

// Regression test for a real bug: `ProjectScoreDebug.selected` used to
// be `pscore > 0.0` — "scored anything at all" — instead of reflecting
// whether the project actually survived the relative-cutoff selection
// into `tailored.experiences`. In Keyword mode especially, almost
// every project shares at least one common word with the JD, so
// nearly everything showed as "selected" in the debug/manual-selection
// UI regardless of what the real tailored result contained.
#[test]
fn debug_scores_selected_matches_what_actually_survived_into_tailored_result() {
    let cv = LifetimeCV {
        experiences: vec![Experience {
            id: "e1".to_string(),
            company: "Acme".to_string(),
            projects: vec![
                ExperienceProject {
                    id: "strong".to_string(),
                    name: LocalizedText::same("Kubernetes Platform"),
                    bullets: vec![LocalizedText::same(
                        "built kubernetes rust postgresql platform",
                    )],
                    ..Default::default()
                },
                ExperienceProject {
                    id: "weak".to_string(),
                    // Shares only the word "platform" with the JD/strong
                    // project — nonzero score, but should not clear the
                    // relative cutoff against the strong project.
                    name: LocalizedText::same("Unrelated Platform Work"),
                    bullets: vec![LocalizedText::same(
                        "unrelated legacy mainframe cobol batch platform",
                    )],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let jd = "Rust developer with Kubernetes and PostgreSQL platform experience";
    let result = tailor_cv(&cv, jd);

    let tailored_project_ids: HashSet<&str> = result
        .tailored
        .experiences
        .iter()
        .flat_map(|e| e.projects.iter())
        .map(|p| p.id.as_str())
        .collect();
    assert!(
        tailored_project_ids.contains("strong"),
        "the clearly relevant project should survive into the tailored result"
    );
    assert!(
        !tailored_project_ids.contains("weak"),
        "the weakly-overlapping project should NOT survive the cutoff"
    );

    let debug_projects: Vec<&ProjectScoreDebug> = result
        .debug_scores
        .iter()
        .flat_map(|e| e.projects.iter())
        .collect();
    let strong_debug = debug_projects.iter().find(|p| p.id == "strong").unwrap();
    let weak_debug = debug_projects.iter().find(|p| p.id == "weak").unwrap();

    assert!(
        strong_debug.selected,
        "strong project's debug flag should be true"
    );
    assert!(
        weak_debug.score > 0.0,
        "fixture sanity check: weak project must have SOME nonzero score \
         for this test to actually exercise the bug"
    );
    assert!(
        !weak_debug.selected,
        "weak project scored > 0.0 but did not survive the cutoff — its \
         debug `selected` flag must be false, not true"
    );
}

#[test]
fn tailor_matched_keywords_are_actually_in_cv() {
    let cv = fixture_cv();
    let jd = "Rust developer with PostgreSQL and Kubernetes experience";
    let result = tailor_cv(&cv, jd);

    let cv_text = cv.all_text().to_lowercase();
    for kw in &result.tailored.matched_keywords {
        assert!(
            cv_text.contains(kw.as_str()),
            "Matched keyword '{}' must appear in CV text",
            kw
        );
    }
}

#[test]
fn tailor_missing_keywords_not_in_cv() {
    let cv = fixture_cv();
    let jd = "Senior Golang developer needed with Terraform and Vault expertise";
    let result = tailor_cv(&cv, jd);

    let cv_text = cv.all_text().to_lowercase();
    // At least one of the JD-specific keywords should be flagged as missing
    let any_gap = result
        .tailored
        .missing_keywords
        .iter()
        .any(|kw| !cv_text.contains(kw.as_str()));
    assert!(
        any_gap,
        "Expected at least one keyword to be missing from CV"
    );
}

#[test]
fn tailor_education_always_included() {
    let mut cv = fixture_cv();
    cv.education.push(Education {
        id: "edu-1".to_string(),
        institution: "MIT".to_string(),
        degree: LocalizedText::same("MSc"),
        field: LocalizedText::same("Computer Science"),
        start_year: "2017".to_string(),
        end_year: "2019".to_string(),
        achievements: vec![],
    });

    // Completely unrelated JD
    let jd = "Looking for a pastry chef with baking and confectionery skills";
    let result = tailor_cv(&cv, jd);
    assert_eq!(
        result.tailored.education.len(),
        1,
        "Education should always be included"
    );
}

#[test]
fn tailor_unrelated_projects_excluded() {
    let cv = fixture_cv(); // project is about Rust/Dioxus
    let jd = "Java Spring Boot developer for enterprise banking application";
    let result = tailor_cv(&cv, jd);
    // The Rust/Dioxus project should not score against a Java JD
    assert!(
        result.tailored.projects.is_empty(),
        "Unrelated project should be filtered out"
    );
}

#[test]
fn tailor_empty_jd_does_not_panic() {
    let cv = fixture_cv();
    let result = tailor_cv(&cv, "");
    assert_eq!(result.tailored.match_score, 0.0);
    assert!(result.tailored.matched_keywords.is_empty());
}

#[test]
fn tailor_empty_cv_does_not_panic() {
    let cv = LifetimeCV::default();
    let result = tailor_cv(&cv, "Rust developer needed for distributed systems work");
    assert!(result.tailored.experiences.is_empty());
    assert!(result.tailored.skills.is_empty());
}

#[test]
fn tailor_top_keywords_capped_at_thirty() {
    let cv = fixture_cv();
    // Generate a JD with many unique high-freq keywords
    let jd = (0..50)
        .map(|i| format!("keyword{i} keyword{i} "))
        .collect::<String>();
    let result = tailor_cv(&cv, &jd);
    assert!(
        result.top_keywords.len() <= 30,
        "top_keywords should be capped at 30, got {}",
        result.top_keywords.len()
    );
}

// ── tailor_cv_with_scorer ─────────────────────────────────────────────────

fn scorer_keyword() -> crate::services::score::Scorer {
    crate::services::score::Scorer::new(crate::services::score::ScoreMode::Keyword)
}

// One experience that matches the JD's only keyword, one that doesn't,
// plus two skills (one relevant, one not). Used to exercise the
// match_score weighted blend and the experience/skill selection.
fn scorer_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        skills: vec![
            Skill {
                id: "s-rust".to_string(),
                name: "rust".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-zz".to_string(),
                name: "zz".to_string(),
                ..Default::default()
            },
        ],
        experiences: vec![
            Experience {
                id: "e-strong".to_string(),
                company: "Alpha".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("rust systems")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-weak".to_string(),
                company: "Beta".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("accounting")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_scorer_match_score_mixes_relevant_experience_and_skill() {
    let cv = scorer_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    // exp_scores = [1.0, 0.0] → mean 0.5 (experience component)
    // skill_scores = [1.0, 0.0] → only >0 included → mean_skill 1.0
    // weighted: (0.5*0.85 + 1.0*0.15) / 1.0 = 0.575
    let expected = 0.5 * 0.85 + 1.0 * 0.15;
    assert!(
        (result.tailored.match_score - expected).abs() < 1e-5,
        "expected {expected}, got {}",
        result.tailored.match_score
    );
}

#[test]
fn tailor_scorer_match_score_excludes_zero_scored_skills_from_skill_mean() {
    // The mean-skill component must average only the skills that scored
    // > 0. With a zero-scoring sibling skill present (s-zz), including it
    // in the mean would halve that component (1.0 → 0.5).
    let cv = scorer_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    // If the zero skill leaked into the skill mean: 0.5*0.85 + 0.5*0.15 = 0.5
    assert!(
        (result.tailored.match_score - 0.575).abs() < 1e-5,
        "got {}",
        result.tailored.match_score
    );
}

#[test]
fn tailor_scorer_empty_cv_has_zero_match_score() {
    // No experiences and no skills → nothing to weight → must be a clean
    // 0.0 (never NaN from a 0/0 division, never a phantom nonzero).
    let cv = LifetimeCV::default();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    assert_eq!(result.tailored.match_score, 0.0);
    assert!(result.tailored.experiences.is_empty());
    assert!(result.tailored.skills.is_empty());
}

#[test]
fn tailor_scorer_selects_relevant_experiences_and_marks_debug_scores() {
    let cv = scorer_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    // Both experiences are necessarily kept by the "at least 2" rule, but
    // the debug scores must reflect the real relevance (strong=1.0 kept by
    // cutoff; weak=0.0 kept only via the minimum-2 fallback).
    let ids: Vec<&str> = result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert!(ids.contains(&"e-strong"));
    assert!(
        ids.contains(&"e-weak"),
        "at least 2 experiences kept, got {ids:?}"
    );
    let strong = result
        .debug_scores
        .iter()
        .find(|d| d.experience_id == "e-strong")
        .expect("strong debug entry");
    let weak = result
        .debug_scores
        .iter()
        .find(|d| d.experience_id == "e-weak")
        .expect("weak debug entry");
    assert!(strong.selected);
    assert!(
        weak.selected,
        "weak kept via minimum-2 fallback, not via cutoff"
    );
    assert!(strong.score > weak.score);
}

// ── project filtering (tailor_cv) ────────────────────────────────────────
//
// One experience with projects of mixed relevance (so the fixed-fraction
// cutoff actually trims), plus one experience whose projects all score 0
// (to exercise the all-zero corner where the `> 0.0` guard is the thing
// being tested). Both experiences are kept: the relevant one by the score
// cutoff, the zero one via the minimum-2 fallback.
fn project_filter_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        experiences: vec![
            Experience {
                id: "exp-mixed".to_string(),
                company: "Alpha".to_string(),
                projects: vec![
                    ExperienceProject {
                        bullets: vec![LT::same("rust systems")],
                        ..Default::default()
                    },
                    ExperienceProject {
                        bullets: vec![LT::same("rust embedded code")],
                        ..Default::default()
                    },
                    ExperienceProject {
                        bullets: vec![LT::same("accounting reports")],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            Experience {
                id: "exp-zero".to_string(),
                company: "Beta".to_string(),
                projects: vec![
                    ExperienceProject {
                        bullets: vec![LT::same("accounting")],
                        ..Default::default()
                    },
                    ExperienceProject {
                        bullets: vec![LT::same("bookkeeping")],
                        ..Default::default()
                    },
                    ExperienceProject {
                        bullets: vec![LT::same("taxes")],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_project_filtering_trims_neutral_and_collapses_all_zero() {
    let cv = project_filter_fixture();
    let result = tailor_cv(&cv, "rust");
    let by_id = |id: &str| {
        result
            .tailored
            .experiences
            .iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("missing experience {id}"))
    };
    let mixed = by_id("exp-mixed");
    assert_eq!(mixed.projects.len(), 2, "neutral project must be trimmed");
    let mixed_text: Vec<String> = mixed
        .projects
        .iter()
        .map(|p| {
            p.bullets
                .iter()
                .map(|b| b.en.clone())
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect();
    assert!(
        mixed_text.iter().all(|t| t.contains("rust")),
        "only rust-relevant projects may survive, got {mixed_text:?}"
    );
    let zero = by_id("exp-zero");
    assert_eq!(
        zero.projects.len(),
        1,
        "all-zero-scoring projects must collapse to the single best (keep-best fallback)"
    );
}

#[test]
fn tailor_scorer_project_filtering_trims_neutral_and_collapses_all_zero() {
    let cv = project_filter_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    let by_id = |id: &str| {
        result
            .tailored
            .experiences
            .iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("missing experience {id}"))
    };
    let mixed = by_id("exp-mixed");
    assert_eq!(mixed.projects.len(), 2, "neutral project must be trimmed");
    let zero = by_id("exp-zero");
    assert_eq!(
        zero.projects.len(),
        1,
        "all-zero projects must collapse to one"
    );
}

// ── experience-level boundary (tailor_cv / tailor_cv_with_scorer) ───────
//
// Three experiences of strictly decreasing relevance to the "rust" JD:
// a strong one, a mid one, and a zero one. With exactly two passing the
// cutoff, the `selected_ids.len() < 2` minimum-two fallback must NOT fire
// (flipping `< 2` to `<= 2` would wrongly add the zero-scoring third).
fn three_tier_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        skills: vec![Skill {
            id: "s-rust".to_string(),
            name: "rust".to_string(),
            ..Default::default()
        }],
        experiences: vec![
            Experience {
                id: "e-strong".to_string(),
                company: "Alpha".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![
                        LT::same("rust systems concurrency performance"),
                        LT::same("rust architecture"),
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-mid".to_string(),
                company: "Beta".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("built rust tooling")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-zero".to_string(),
                company: "Gamma".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("accounting bookkeeping taxes")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn assert_exactly_two_without_fallback(result: &TailorResult) {
    let ids: Vec<&str> = result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        2,
        "exactly the strong and mid experiences must be kept, got {ids:?}"
    );
    assert!(
        ids.contains(&"e-strong") && ids.contains(&"e-mid"),
        "strong and mid must both be kept, got {ids:?}"
    );
    assert!(
        !ids.contains(&"e-zero"),
        "zero-scoring experience must be excluded once 2 clear the cutoff, got {ids:?}"
    );
}

#[test]
fn tailor_exactly_two_selected_does_not_trigger_min_two_fallback() {
    let cv = three_tier_fixture();
    let result = tailor_cv(&cv, "rust");
    assert_exactly_two_without_fallback(&result);
}

#[test]
fn tailor_scorer_exactly_two_selected_does_not_trigger_min_two_fallback() {
    let cv = three_tier_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    assert_exactly_two_without_fallback(&result);
}

// A skills-only CV drives `match_score` through the single-category branch
// where the weighting denominators are < 1.0, so a `sum / ratio` is
// distinguishable from a `sum * ratio` (they coincide only when the weight
// total is exactly 1.0). Two zero-scoring skills are also present to force
// the `> 0.0` skill guard to exclude them from the mean.
fn skills_only_fixture() -> LifetimeCV {
    LifetimeCV {
        skills: vec![
            Skill {
                id: "s-rust".to_string(),
                name: "rust engineer".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-zz1".to_string(),
                name: "accounting".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-zz2".to_string(),
                name: "bookkeeping".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_scorer_skills_only_match_score_uses_fractional_weight_denominator() {
    let cv = skills_only_fixture();
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "rust", &mut scorer, None);
    // Only the rust skill scores > 0, and it scores 1.0 (its single term
    // is the only keyword term in the CV corpus). skill-only path:
    // mean_skill = 1.0, weight_total = 0.15 (skill weight).
    // match_score = 1.0 * 0.15 / 0.15 = 1.0.
    // A `*`/`%` flip of the division (1.0*0.15 vs 1.0%0.15) would NOT
    // equal 1.0, so asserting the exact 1.0 discriminates the division
    // mutant (whose result would be clamped/blended differently).
    assert_eq!(
        result.tailored.match_score, 1.0,
        "skills-only score must be 1.0, got {}",
        result.tailored.match_score
    );
    // The unrelated zero-scoring skills (default-level = Intermediate)
    // are dropped by the tailored-skills filter — only the matched one
    // survives, and it stays the leading skill.
    let names: Vec<&str> = result
        .tailored
        .skills
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["rust engineer"]);
}

// Keyword mode applies NO mean floor to the experience cutoff, so a
// mid-scoring experience survives; the same CV under Hybrid mode (which
// applies the mean floor) trims it. This discriminates the
// `scorer.mode != Keyword` gating from an unconditional application.
fn cluster_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        skills: vec![
            Skill {
                id: "s-rust".to_string(),
                name: "rust".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-zz".to_string(),
                name: "accounting".to_string(),
                ..Default::default()
            },
        ],
        experiences: vec![
            Experience {
                id: "e-top".to_string(),
                company: "Alpha".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same(
                        "rust systems concurrency performance architecture",
                    )],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-mid".to_string(),
                company: "Beta".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("rust services")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-low".to_string(),
                company: "Gamma".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("rust")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_scorer_mean_floor_only_applies_outside_keyword_mode() {
    use crate::services::score::{ScoreMode, Scorer};
    let cv = cluster_fixture();
    let mut kw = Scorer::new(ScoreMode::Keyword);
    let mut hybrid = Scorer::new(ScoreMode::Hybrid);
    let kw_result = tailor_cv_with_scorer(&cv, "rust", &mut kw, None);
    let hy_result = tailor_cv_with_scorer(&cv, "rust", &mut hybrid, None);
    let kw_ids: Vec<&str> = kw_result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    let hy_ids: Vec<&str> = hy_result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert!(
        kw_ids.len() >= hy_ids.len(),
        "Keyword mode must keep at least as many experiences as Hybrid: kw={kw_ids:?} hy={hy_ids:?}"
    );
}

#[test]
fn tailor_plain_zero_scoring_experience_excluded_when_two_pass() {
    let cv = three_tier_fixture();
    let result = tailor_cv(&cv, "rust");
    assert_exactly_two_without_fallback(&result);
}

// ── `tailor_cv`'s inline mean-floor cutoff (mean_score / cutoff math) ─────
//
// Deliberately declared out of score order (mid, low, top1, top2) so that
// the experience-selection fallback (`selected_ids.len() < 2` — see
// below) would pick a visibly *different* pair (mid, low) than the
// correct relevance-based pair (top1, top2) if it wrongly fired. That
// makes this fixture sensitive to several distinct mutations at once:
//   - `mean_score = sum / len` corrupted to `sum * len` or `sum % len`
//     inflates the cutoff far past every real score (all scores are
//     bounded in [0, 1], but a product/modulo of them against `len` is
//     not), so nothing clears it and the fallback wrongly fires.
//   - `max_score * REL_THRESHOLD` corrupted to `+` or `/` similarly
//     produces a fixed-cutoff component far outside [0, 1], forcing the
//     same wrong fallback.
//   - the experience filter's `&&` loosened to `||` wrongly keeps `mid`
//     (it clears the `> 0.0` guard even though it doesn't clear the
//     cutoff).
//   - the filter's first `> 0.0` flipped to `==`/`<` wrongly excludes
//     everything (top1/top2 are nonzero, so they fail `== 0.0`/`< 0.0`),
//     which also wrongly fires the fallback.
//   - the filter's `>= cutoff` flipped to `< cutoff` inverts who passes,
//     wrongly keeping `mid` and dropping `top1`/`top2`.
//   - the fallback guard `< 2` loosened to `<= 2` wrongly re-triggers
//     even though exactly 2 experiences already passed, pulling in the
//     first two *declared* experiences (mid, low) on top of the correct
//     pair.
// Any one of these collapses the result away from the exact {top1, top2}
// pair asserted below.
fn mean_floor_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        experiences: vec![
            Experience {
                id: "e-mid".to_string(),
                company: "Mid".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-low".to_string(),
                company: "Low".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("delta")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-top1".to_string(),
                company: "Top1".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-top2".to_string(),
                company: "Top2".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_plain_mean_floor_cutoff_selects_exactly_the_full_matches() {
    let cv = mean_floor_fixture();
    let result = tailor_cv(&cv, "alpha beta gamma");
    let mut ids: Vec<&str> = result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["e-top1", "e-top2"],
        "mean-floor cutoff must exclude the partial ('e-mid') and \
         non-matching ('e-low') experiences, got {ids:?}"
    );
}

// Pins the `> 0.0` half of the experience filter specifically against a
// `>= 0.0` mutation. With every experience scoring exactly 0.0 (no JD
// keyword present anywhere), `max_score` and `cutoff` are both 0.0, so a
// `>= 0.0` guard would wrongly let every zero-scoring experience through
// (0.0 >= 0.0 is true), instead of correctly falling through to the
// "keep first two declared" fallback.
#[test]
fn tailor_plain_all_zero_scores_uses_fallback_not_a_zero_cutoff_pass() {
    use crate::models::cv::LocalizedText as LT;
    let cv = LifetimeCV {
        experiences: vec![
            Experience {
                id: "e1".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("accounting")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e2".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("bookkeeping")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e3".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("taxes")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let result = tailor_cv(&cv, "rust");
    assert_eq!(
        result.tailored.experiences.len(),
        2,
        "with a 0.0 cutoff, `> 0.0` must still exclude every zero-scoring \
         experience and fall back to keeping exactly the first two \
         declared, not let all of them through"
    );
}

// ── `tailor_cv`'s top-level project filter (`score > 0.0`) ────────────────
#[test]
fn tailor_plain_top_level_projects_filtered_by_positive_score() {
    let cv = LifetimeCV {
        projects: vec![
            Project {
                id: "p-match".to_string(),
                name: "alpha project".to_string(),
                ..Default::default()
            },
            Project {
                id: "p-nomatch".to_string(),
                name: "unrelated".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let result = tailor_cv(&cv, "alpha");
    let ids: Vec<&str> = result
        .tailored
        .projects
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["p-match"],
        "only the positively-scoring project must survive; a `>` flipped \
         to `<` would drop every project (scores are never negative), \
         got {ids:?}"
    );
}

// ── `tailor_cv_with_scorer`'s mode gate on the experience mean floor ──────
//
// Rather than hand-predicting exact scores (stemming/bigram-trigram
// term extraction and TF-IDF weighting make that unreliable to do by
// hand — an earlier version of this test tried and got it wrong), this
// derives the expected floor-vs-no-floor selection by calling the same
// `score_experience`/`select_by_relative_cutoff` primitives
// `tailor_cv_with_scorer` itself calls, independent of its mode-gate
// wiring (the one piece actually under test). It self-checks that the
// fixture genuinely exercises a floor-vs-no-floor difference before
// asserting anything about `tailor_cv_with_scorer`, so a fixture that
// stops producing that difference fails loudly with a diagnosis rather
// than silently passing (or wrongly failing) mutant-blind.
//
// Exactly 2 full matches ("top") and 2 identical partial matches
// ("mid", missing only the JD's last word) is deliberate, not just a
// round number: with N identical top scores (all == max) and M
// identical mid scores (all == some value < max), the mean is exactly
// the count-weighted average of max and mid, which — for ANY mid <
// max — always lands strictly between them. So "mid" is guaranteed to
// sit below the mean-floor cutoff without needing to predict its exact
// score by hand (multi-word term extraction and TF-IDF weighting make
// that unreliable — see the two earlier, wrong attempts at this test).
// The only thing that must hold empirically is that "mid" (missing
// just one of five words) still clears the much lower *fixed* cutoff
// (0.5 * max) — checked by the self-check assertion below rather than
// assumed.
fn mode_gate_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        experiences: vec![
            // Declared mid-first, top-last: deliberate, not arbitrary —
            // `tailor_scorer_exactly_two_selected_does_not_trigger_min_two_fallback`
            // below relies on the fallback's "first two *declared*"
            // pick being visibly wrong (mid1, mid2) if it wrongly
            // fires, which only works if the correct answer (top1,
            // top2) ISN'T also the first two in declaration order.
            Experience {
                id: "e-mid1".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma delta")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-mid2".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma delta")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-top1".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma delta epsilon")],
                    ..Default::default()
                }],
                ..Default::default()
            },
            Experience {
                id: "e-top2".to_string(),
                projects: vec![ExperienceProject {
                    bullets: vec![LT::same("alpha beta gamma delta epsilon")],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

#[test]
fn tailor_scorer_keyword_mode_keeps_strictly_more_than_hybrid_mode() {
    use crate::services::score::{ScoreMode, Scorer};
    let cv = mode_gate_fixture();
    let jd = "alpha beta gamma delta epsilon";

    // Reconstruct the real per-experience keyword scores exactly the
    // way `tailor_cv_with_scorer` does internally (same top_keywords
    // and corpus-built Idf), then apply `select_by_relative_cutoff`
    // with the floor on and off directly — this exercises the shared
    // cutoff helper (already covered by its own dedicated tests) but
    // NOT the mode-gate line inside `tailor_cv_with_scorer`, keeping
    // that one thing genuinely independent of what's under test.
    let keywords = extract_keywords(jd);
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();
    let mut probe = Scorer::new(ScoreMode::Keyword);
    probe.idf = Idf::build(&cv_documents(&cv));
    let scores: Vec<f32> = cv
        .experiences
        .iter()
        .map(|e| probe.score_experience(e, &top_keywords, None, &cv.skills))
        .collect();
    const REL_THRESHOLD: f32 = 0.5;
    let no_floor = select_by_relative_cutoff(&scores, REL_THRESHOLD, false);
    let with_floor = select_by_relative_cutoff(&scores, REL_THRESHOLD, true);
    assert!(
        no_floor.len() > with_floor.len() && with_floor.len() >= 2,
        "fixture must exercise a floor-vs-no-floor difference without \
         either side needing the separate min-two fallback (which would \
         make this test's comparison meaningless): scores={scores:?} \
         no_floor={no_floor:?} with_floor={with_floor:?} — adjust the \
         fixture if this fails"
    );

    // With that confirmed, `tailor_cv_with_scorer` in Keyword mode
    // (no floor) must select strictly more experiences than in Hybrid
    // mode (floor applied) — Hybrid's embedding term is 0 with no
    // engine/jd_embedding here, which uniformly scales every keyword
    // score by the same constant and so cannot itself change which
    // experiences clear the cutoff; only the mode gate can.
    let mut kw = Scorer::new(ScoreMode::Keyword);
    let mut hybrid = Scorer::new(ScoreMode::Hybrid);
    let kw_result = tailor_cv_with_scorer(&cv, jd, &mut kw, None);
    let hy_result = tailor_cv_with_scorer(&cv, jd, &mut hybrid, None);
    let kw_ids: HashSet<&str> = kw_result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    let hy_ids: HashSet<&str> = hy_result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert!(
        kw_ids.len() > hy_ids.len(),
        "Keyword mode (no mean floor) must keep strictly more \
         experiences than Hybrid mode (mean floor applied) here: \
         kw={kw_ids:?} hy={hy_ids:?}"
    );
}

// ── `tailor_cv_with_scorer`'s min-two fallback guard (`< 2`) ──────────────
//
// There's already a `tailor_scorer_exactly_two_selected_does_not_trigger_
// min_two_fallback` test (Keyword mode, `three_tier_fixture`) covering
// this same line — this one is kept alongside it, not merged in, because
// it exercises the same guard via a genuinely different path: Hybrid
// mode's mean floor (rather than Keyword's fixed-fraction cutoff)
// landing on exactly 2, using `mode_gate_fixture` in a mode the other
// test doesn't touch.
//
// Reuses `mode_gate_fixture` in Hybrid mode, where the mean floor
// already trims it to exactly the two full matches (top1, top2) — see
// the self-check in the test above. Exactly 2 is the boundary value: a
// `< 2` guard flipped to `<= 2` would wrongly re-fire here even though
// two experiences already legitimately passed, pulling in the fixture's
// first two *declared* experiences (mid1, mid2 — see the comment on
// `mode_gate_fixture` for why it's ordered that way) on top of the
// correct pair.
#[test]
fn tailor_scorer_hybrid_exactly_two_selected_does_not_trigger_min_two_fallback() {
    use crate::services::score::{ScoreMode, Scorer};
    let cv = mode_gate_fixture();
    let jd = "alpha beta gamma delta epsilon";
    let mut hybrid = Scorer::new(ScoreMode::Hybrid);
    let result = tailor_cv_with_scorer(&cv, jd, &mut hybrid, None);
    let mut ids: Vec<&str> = result
        .tailored
        .experiences
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["e-top1", "e-top2"],
        "exactly two experiences pass the mean-floor cutoff; the `< 2` \
         fallback guard must not re-fire and pull in extra \
         declared-order experiences, got {ids:?}"
    );
}

// ── `tailor_cv_with_scorer`'s mode gate on the *project*-level mean floor ─
//
// Same "always strictly between" trick as the experience-level mode-gate
// test above, one level down: within a single experience's projects,
// 2 full matches + 2 identical partial matches means the mean is
// guaranteed to sit strictly between them, so "mid" always falls below
// the floor regardless of its exact score. The self-check below still
// verifies empirically that "mid" clears the (higher, 0.7×max) fixed
// cutoff used at the project level before asserting anything about
// `tailor_cv_with_scorer` — an 8-word JD with "mid" missing only the
// last word keeps its fractional loss small, but this is exactly the
// kind of assumption that's gone wrong twice already in this file, so
// it isn't trusted blindly here either.
fn project_mode_gate_fixture() -> LifetimeCV {
    use crate::models::cv::LocalizedText as LT;
    LifetimeCV {
        experiences: vec![Experience {
            id: "e1".to_string(),
            // >1 project is required for the project-level cutoff loop
            // to run at all (`if exp.projects.len() <= 1 { continue }`).
            projects: vec![
                ExperienceProject {
                    id: "p-top1".to_string(),
                    bullets: vec![LT::same(
                        "alpha beta gamma delta epsilon zeta eta theta",
                    )],
                    ..Default::default()
                },
                ExperienceProject {
                    id: "p-top2".to_string(),
                    bullets: vec![LT::same(
                        "alpha beta gamma delta epsilon zeta eta theta",
                    )],
                    ..Default::default()
                },
                ExperienceProject {
                    id: "p-mid1".to_string(),
                    bullets: vec![LT::same("alpha beta gamma delta epsilon zeta eta")],
                    ..Default::default()
                },
                ExperienceProject {
                    id: "p-mid2".to_string(),
                    bullets: vec![LT::same("alpha beta gamma delta epsilon zeta eta")],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn tailor_scorer_project_keyword_mode_keeps_strictly_more_than_hybrid_mode() {
    use crate::services::score::{ScoreMode, Scorer};
    let cv = project_mode_gate_fixture();
    let jd = "alpha beta gamma delta epsilon zeta eta theta";
    const PROJECT_REL_THRESHOLD: f32 = 0.7;

    // Reconstruct the real per-project keyword scores the same way
    // `tailor_cv_with_scorer` does internally, independent of its
    // project-level mode gate (the one thing actually under test).
    let keywords = extract_keywords(jd);
    let top_keywords: Vec<(String, usize)> = keywords.iter().take(40).cloned().collect();
    let mut probe = Scorer::new(ScoreMode::Keyword);
    probe.idf = Idf::build(&cv_documents(&cv));
    let exp = &cv.experiences[0];
    let shared_tools = pooled_tools(&exp.projects, &cv.skills);
    let scores: Vec<f32> = exp
        .projects
        .iter()
        .map(|p| probe.score_experience_project(p, &top_keywords, None, &shared_tools))
        .collect();
    let no_floor = select_by_relative_cutoff(&scores, PROJECT_REL_THRESHOLD, false);
    let with_floor = select_by_relative_cutoff(&scores, PROJECT_REL_THRESHOLD, true);
    assert!(
        no_floor.len() > with_floor.len(),
        "fixture must exercise a project-level floor-vs-no-floor \
         difference: scores={scores:?} no_floor={no_floor:?} \
         with_floor={with_floor:?} — adjust the fixture if this fails"
    );

    let mut kw = Scorer::new(ScoreMode::Keyword);
    let mut hybrid = Scorer::new(ScoreMode::Hybrid);
    let kw_result = tailor_cv_with_scorer(&cv, jd, &mut kw, None);
    let hy_result = tailor_cv_with_scorer(&cv, jd, &mut hybrid, None);
    let kw_proj_ids: HashSet<&str> = kw_result.tailored.experiences[0]
        .projects
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    let hy_proj_ids: HashSet<&str> = hy_result.tailored.experiences[0]
        .projects
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert!(
        kw_proj_ids.len() > hy_proj_ids.len(),
        "Keyword mode (no project-level mean floor) must keep strictly \
         more projects than Hybrid mode (floor applied) here: \
         kw={kw_proj_ids:?} hy={hy_proj_ids:?}"
    );
}

// ── `tailor_cv_with_scorer`'s top-level project filter (`score > 0.0`) ────
#[test]
fn tailor_scorer_top_level_projects_filtered_by_positive_score() {
    let cv = LifetimeCV {
        projects: vec![
            Project {
                id: "p-match".to_string(),
                name: "alpha project".to_string(),
                ..Default::default()
            },
            Project {
                id: "p-nomatch".to_string(),
                name: "unrelated".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, "alpha", &mut scorer, None);
    let ids: Vec<&str> = result
        .tailored
        .projects
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["p-match"],
        "only the positively-scoring project must survive; a `>` flipped \
         to `<` would drop every project (scores are never negative), \
         got {ids:?}"
    );
}

// ── `mean_skill_score` must be a true average, not a product ─────────────
//
// With exactly one contributing skill, `sum / len` and `sum * len`
// coincide (dividing or multiplying by 1 is a no-op), so a single-skill
// fixture can't distinguish them — this needs at least two skills with
// nonzero, unequal-looking scores. Rather than hand-predicting exact
// scores (stemming/synonym normalization and multi-word term extraction
// make that unreliable by hand — an earlier version of this test tried
// and got the wrong constant), this derives the expected mean by calling
// `score_skill` directly with the same keywords/Idf construction
// `tailor_cv_with_scorer` uses internally, then compares that
// independently-derived mean against the actual `match_score`. With no
// experiences or projects in this fixture, skills are the only
// contributor to `match_score`, so it equals `mean_skill_score` exactly.
#[test]
fn tailor_scorer_mean_skill_score_is_a_true_average_not_a_product() {
    let cv = LifetimeCV {
        skills: vec![
            Skill {
                id: "s-alpha".to_string(),
                name: "alpha".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-beta".to_string(),
                name: "beta".to_string(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let jd = "alpha beta";
    let keywords = extract_keywords(jd);
    let idf = Idf::build(&cv_documents(&cv));
    let scores: Vec<f32> = cv
        .skills
        .iter()
        .map(|s| score_skill(s, &keywords, &idf))
        .collect();
    assert!(
        scores.iter().all(|&s| s > 0.0),
        "fixture must produce nonzero per-skill scores to test with: {scores:?}"
    );
    let expected_mean = scores.iter().sum::<f32>() / scores.len() as f32;

    let mut scorer = scorer_keyword();
    let result = tailor_cv_with_scorer(&cv, jd, &mut scorer, None);
    assert!(
        (result.tailored.match_score - expected_mean).abs() < 1e-4,
        "match_score must equal the true average of per-skill scores \
         ({expected_mean}), got {} (per-skill scores were {scores:?})",
        result.tailored.match_score
    );
}

// ── `cv_documents` ─────────────────────────────────────────────────────
//
// Pins that it actually walks the CV and returns one real document per
// scorable block, not a stub. `vec![]`, `vec![vec![]]`, and
// `vec![vec![String::new()]]` are all caught by the emptiness/content
// checks below; `vec![vec!["xyzzy".into()]]` is caught by checking the
// returned terms actually reflect the CV's own text, not fixed filler.
#[test]
fn cv_documents_returns_one_real_document_per_scorable_block() {
    let cv = fixture_cv();
    let docs = cv_documents(&cv);
    // fixture_cv has 2 experience-project bullets-blocks (one per
    // experience), 1 top-level project, and 3 skills = 6 scorable
    // blocks.
    assert_eq!(docs.len(), 6, "expected one document per scorable block, got {docs:?}");
    assert!(
        docs.iter().all(|d| !d.is_empty()),
        "every document must contain real extracted terms, not be empty: {docs:?}"
    );
    assert!(
        docs.iter()
            .any(|d| d.iter().any(|t| t.contains("rust") || t == "rust")),
        "documents must reflect the CV's actual text, not fixed filler: {docs:?}"
    );
    assert!(
        docs.iter().all(|d| !d.iter().any(|t| t == "xyzzy")),
        "documents must not contain unrelated filler text: {docs:?}"
    );
}
