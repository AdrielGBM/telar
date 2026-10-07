use super::*;
use crate::{Author, License};

fn set(name: &str, spdx: Option<&str>) -> SetInfo {
    SetInfo {
        name: Some(name.to_string()),
        license: spdx.map(|spdx| License {
            title: Some(spdx.to_string()),
            spdx: Some(spdx.to_string()),
            url: None,
        }),
        ..SetInfo::default()
    }
}

#[test]
fn licences_are_classified_by_what_they_ask() {
    assert_eq!(
        LicenseClass::of(Some("CC0-1.0")),
        LicenseClass::PublicDomain
    );
    assert_eq!(LicenseClass::of(Some("mit")), LicenseClass::Permissive);
    assert_eq!(
        LicenseClass::of(Some("Apache-2.0")),
        LicenseClass::Permissive
    );
    assert_eq!(
        LicenseClass::of(Some("CC-BY-4.0")),
        LicenseClass::Attribution
    );
    assert_eq!(
        LicenseClass::of(Some("CC-BY-SA-4.0")),
        LicenseClass::Copyleft
    );
    assert_eq!(
        LicenseClass::of(Some("GPL-3.0-only")),
        LicenseClass::Copyleft
    );
    assert_eq!(LicenseClass::of(Some("OFL-1.1")), LicenseClass::Copyleft);
    assert_eq!(
        LicenseClass::of(Some("CC-BY-NC-4.0")),
        LicenseClass::Restricted
    );
    assert_eq!(LicenseClass::of(Some("Custom")), LicenseClass::Unknown);
    assert_eq!(LicenseClass::of(None), LicenseClass::Unknown);
}

#[test]
fn permissive_sets_need_no_decision() {
    let policy = LicensePolicy {
        unlisted: OnUnlisted::Fail,
        ..LicensePolicy::default()
    };
    assert_eq!(
        policy.judge("mdi", Some(&set("MDI", Some("Apache-2.0"))), false),
        Verdict::Accepted
    );
    assert_eq!(
        policy.judge("simple-icons", Some(&set("SI", Some("CC0-1.0"))), false),
        Verdict::Accepted
    );
}

#[test]
fn an_attribution_set_warns_by_default() {
    let verdict = LicensePolicy::default().judge(
        "twemoji",
        Some(&set("Twitter Emoji", Some("CC-BY-4.0"))),
        false,
    );
    let Verdict::Warn(message) = verdict else {
        panic!("expected a warning, got {verdict:?}")
    };
    assert!(message.contains("`twemoji` (Twitter Emoji)"), "{message}");
    assert!(message.contains("crediting its author"), "{message}");
    assert!(message.contains("\"CC-BY-4.0\""), "{message}");
}

#[test]
fn a_copyleft_set_fails_when_unlisted_sets_fail() {
    let policy = LicensePolicy {
        allow: Vec::new(),
        unlisted: OnUnlisted::Fail,
    };
    let verdict = policy.judge("gpl-set", Some(&set("GPL Set", Some("GPL-3.0"))), false);
    let Verdict::Fail(message) = verdict else {
        panic!("expected a failure, got {verdict:?}")
    };
    assert!(message.contains("copyleft"), "{message}");
}

#[test]
fn the_allowlist_accepts_a_licence_or_one_set() {
    let by_licence = LicensePolicy {
        allow: vec!["cc-by-4.0".to_string()],
        unlisted: OnUnlisted::Fail,
    };
    assert_eq!(
        by_licence.judge("twemoji", Some(&set("T", Some("CC-BY-4.0"))), false),
        Verdict::Accepted
    );
    let by_set = LicensePolicy {
        allow: vec!["mystery".to_string()],
        unlisted: OnUnlisted::Fail,
    };
    assert_eq!(by_set.judge("mystery", None, false), Verdict::Accepted);
    assert!(matches!(
        by_set.judge("other", None, false),
        Verdict::Fail(_)
    ));
}

#[test]
fn a_set_without_a_licence_names_its_prefix_to_accept_it() {
    let Verdict::Warn(message) = LicensePolicy::default().judge("mystery", None, false) else {
        panic!("expected a warning")
    };
    assert!(message.contains("declares no licence"), "{message}");
    assert!(message.contains("add \"mystery\""), "{message}");
}

#[test]
fn the_applications_own_artwork_is_never_judged() {
    let policy = LicensePolicy {
        unlisted: OnUnlisted::Fail,
        ..LicensePolicy::default()
    };
    assert_eq!(policy.judge("app", None, true), Verdict::Accepted);
}

#[test]
fn brand_sets_are_recognised_by_prefix_or_category() {
    assert!(is_brand_set("simple-icons", None));
    let branded = SetInfo {
        category: Some("Brands / Social".to_string()),
        ..SetInfo::default()
    };
    assert!(is_brand_set("my-logos", Some(&branded)));
    assert!(!is_brand_set("mdi", Some(&set("MDI", Some("Apache-2.0")))));
}

#[test]
fn the_notice_lists_each_set_its_licence_and_its_icons() {
    let mdi = SetInfo {
        author: Some(Author {
            name: Some("Pictogrammers".to_string()),
            url: None,
        }),
        ..set("Material Design Icons", Some("Apache-2.0"))
    };
    let text = notice(
        "demo",
        &[
            NoticeSet {
                prefix: "mdi".to_string(),
                set: Some(mdi),
                own: false,
                icons: vec!["account".to_string(), "home".to_string()],
            },
            NoticeSet {
                prefix: "simple-icons".to_string(),
                set: Some(set("Simple Icons", Some("CC0-1.0"))),
                own: false,
                icons: vec!["github".to_string()],
            },
            NoticeSet {
                prefix: "app".to_string(),
                set: None,
                own: true,
                icons: vec!["logo".to_string()],
            },
        ],
    );
    assert!(text.contains("mdi — Material Design Icons\n  Licence: Apache-2.0\n  Author: Pictogrammers\n  Icons: account, home"), "{text}");
    assert!(
        text.contains("simple-icons — Simple Icons\n  Licence: CC0-1.0\n  Trademarks:"),
        "{text}"
    );
    assert!(
        text.contains("app\n  The application's own artwork.\n  Icons: logo"),
        "{text}"
    );
}
