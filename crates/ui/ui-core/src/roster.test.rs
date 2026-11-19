use super::*;

#[test]
fn entries_keep_the_order_they_joined_in_across_withdrawals() {
    let mut roster = Roster::default();
    for key in 0..100u32 {
        roster.push(key, key * 10);
    }
    for key in (0..100u32).filter(|key| key % 3 != 0) {
        assert_eq!(roster.remove(key), Some(key * 10));
    }
    let left: Vec<u32> = roster.iter().copied().collect();
    let expected: Vec<u32> = (0..100u32)
        .filter(|key| key % 3 == 0)
        .map(|key| key * 10)
        .collect();
    assert_eq!(left, expected);
    assert_eq!(roster.get(99), Some(&990));
    assert_eq!(roster.get(98), None);
}

#[test]
fn joining_again_under_a_key_moves_the_entry_to_the_end() {
    let mut roster = Roster::default();
    roster.push("a", 1);
    roster.push("b", 2);
    roster.push("a", 3);
    assert_eq!(
        roster.iter_keyed().collect::<Vec<_>>(),
        vec![("b", &2), ("a", &3)]
    );
}

#[test]
fn an_entry_changed_in_place_is_read_back_changed() {
    let mut roster = Roster::default();
    roster.push(1u8, String::from("one"));
    roster.get_mut(1).unwrap().push('!');
    assert_eq!(roster.get(1).map(String::as_str), Some("one!"));
    assert!(roster.contains(1));
    assert!(!roster.contains(2));
}

#[test]
fn removing_where_hands_back_what_left_in_order() {
    let mut roster = Roster::default();
    for key in 0..10u8 {
        roster.push(key, key);
    }
    assert_eq!(
        roster.remove_where(|value| value % 2 == 1),
        vec![1, 3, 5, 7, 9]
    );
    assert_eq!(roster.take_all(), vec![0, 2, 4, 6, 8]);
    assert_eq!(roster.iter().count(), 0);
}
