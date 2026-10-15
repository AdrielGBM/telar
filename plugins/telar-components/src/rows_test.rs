use telar::{dispose_owner, owner_scope};

use super::*;

fn id() -> FocusId {
    telar::focus::next_id()
}

fn built_in_scope(rows: BuiltRows<u32>, key: u32, row: FocusId) -> telar::OwnerId {
    let scope = owner_scope();
    rows.track(key, row);
    scope.id()
}

#[test]
fn a_built_row_is_named_until_it_is_dropped() {
    let rows = BuiltRows::new();
    let row = id();
    let scope = built_in_scope(rows, 1, row);
    assert_eq!(rows.get(&1), Some(row));
    assert_eq!(rows.get(&2), None);
    dispose_owner(scope);
    assert_eq!(rows.get(&1), None);
}

#[test]
fn a_row_rebuilt_under_its_key_outlives_the_one_it_replaced() {
    let rows = BuiltRows::new();
    let (old, new) = (id(), id());
    let first = built_in_scope(rows, 7, old);
    let second = built_in_scope(rows, 7, new);
    dispose_owner(first);
    assert_eq!(
        rows.get(&7),
        Some(new),
        "the old row's withdrawal leaves the new one"
    );
    dispose_owner(second);
    assert_eq!(rows.get(&7), None);
}
