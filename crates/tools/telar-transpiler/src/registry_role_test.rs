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
