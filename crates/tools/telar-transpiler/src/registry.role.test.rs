use super::*;

/// The transpiler emits a path and the runtime parses a name; they are two tables and they have to agree, or a role an author is offered is one the vocabulary does not have.
#[test]
fn every_spelling_is_one_the_vocabulary_answers_to() {
    for (name, _) in role_values() {
        assert!(
            semantics_core::Role::parse(name).is_some(),
            "`{name}` is offered but the vocabulary does not know it"
        );
    }
}

/// The path a spelling is emitted as names the role the runtime parses that spelling to, so whether the transpiler makes the box a control and what the box turns out to be are one answer.
#[test]
fn every_spelling_is_emitted_as_the_role_it_parses_to() {
    for (name, variant) in role_values() {
        let parsed = semantics_core::Role::parse(name).expect("a known spelling");
        assert_eq!(format!("{parsed:?}"), *variant, "`{name}`");
    }
}

/// What a call may say is exactly the spellings whose role the runtime lets a call lay over an item, so the build never refuses a role the runtime would honour nor emits one it would ignore.
#[test]
fn a_call_is_offered_exactly_the_roles_the_runtime_lets_it_say() {
    let annotatable: Vec<(&str, &str)> = role_values()
        .iter()
        .copied()
        .filter(|(name, _)| {
            semantics_core::Role::parse(name).is_some_and(|role| role.is_annotatable())
        })
        .collect();
    assert_eq!(COMPONENT_ROLE_VALUES, annotatable.as_slice());
}

/// Every role the vocabulary knows that a call may not say is refused with a reason, and every one it may say is not.
#[test]
fn a_role_a_call_may_not_say_is_refused_with_why() {
    for (name, _) in role_values() {
        let refused = refused_component_role(name);
        let allowed = keyword(COMPONENT_ROLE_VALUES, name).is_some();
        assert_eq!(refused.is_none(), allowed, "`{name}`: {refused:?}");
    }
    assert_eq!(refused_component_role("landmark"), None);
}
