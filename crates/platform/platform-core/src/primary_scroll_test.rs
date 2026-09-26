use super::*;

struct Recorded(Cell<(f32, f32)>);

impl PrimaryScroll for Recorded {
    fn offset(&self) -> (f32, f32) {
        self.0.get()
    }

    fn scroll_to(&self, x: f32, y: f32) {
        self.0.set((x, y));
    }
}

fn recorded(at: (f32, f32)) -> Rc<Recorded> {
    Rc::new(Recorded(Cell::new(at)))
}

#[test]
fn nothing_claimed_reads_nothing_and_moves_nothing() {
    assert_eq!(primary_scroll_offset(), None);
    assert!(!scroll_primary_to(0.0, 10.0));
}

#[test]
fn the_claimed_scroll_is_read_and_moved() {
    let scroll = recorded((0.0, 40.0));
    let _claim = claim_primary_scroll(scroll.clone());
    assert_eq!(primary_scroll_offset(), Some((0.0, 40.0)));
    assert!(scroll_primary_to(0.0, 120.0));
    assert_eq!(scroll.offset(), (0.0, 120.0));
}

#[test]
fn dropping_the_claim_lets_go() {
    let claim = claim_primary_scroll(recorded((0.0, 5.0)));
    drop(claim);
    assert_eq!(primary_scroll_offset(), None);
}

#[test]
fn an_older_claim_dropped_late_leaves_the_newer_one() {
    let old = claim_primary_scroll(recorded((0.0, 1.0)));
    let _new = claim_primary_scroll(recorded((0.0, 2.0)));
    drop(old);
    assert_eq!(primary_scroll_offset(), Some((0.0, 2.0)));
}
