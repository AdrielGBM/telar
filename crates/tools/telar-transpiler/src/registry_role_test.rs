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
