mod effect;
mod memo;
mod runtime;
mod signal;

pub use effect::{Effect, create_effect};
pub use memo::{Memo, create_memo};
pub use runtime::{FlushNotifyHandle, batch, set_flush_notify};
pub use signal::{ReadSignal, RwSignal, WriteSignal, create_rw_signal, create_signal};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn signal_get_set() {
        let (count, set_count) = create_signal(0i32);
        assert_eq!(count.get(), 0);
        set_count.set(42);
        assert_eq!(count.get(), 42);
    }

    #[test]
    fn signal_update() {
        let (count, set_count) = create_signal(10i32);
        set_count.update(|v| *v *= 2);
        assert_eq!(count.get(), 20);
    }

    #[test]
    fn signal_with() {
        let (name, _) = create_signal(String::from("rsx"));
        let len = name.with(|s| s.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn rw_signal() {
        let count = create_rw_signal(0i32);
        count.set(10);
        assert_eq!(count.get(), 10);
        count.update(|v| *v += 5);
        assert_eq!(count.get(), 15);
    }

    #[test]
    fn rw_signal_split() {
        let sig = create_rw_signal(0i32);
        let read = sig.read_only();
        let write = sig.write_only();
        write.set(7);
        assert_eq!(read.get(), 7);
    }

    #[test]
    fn effect_runs_immediately() {
        let ran = Rc::new(RefCell::new(false));
        let ran_clone = Rc::clone(&ran);
        let _e = create_effect(move || {
            *ran_clone.borrow_mut() = true;
        });
        assert!(*ran.borrow());
    }

    #[test]
    fn effect_reruns_on_signal_change() {
        let (count, set_count) = create_signal(0i32);
        let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let log_clone = Rc::clone(&log);

        let _e = create_effect(move || {
            log_clone.borrow_mut().push(count.get());
        });

        set_count.set(1);
        set_count.set(2);

        assert_eq!(*log.borrow(), vec![0, 1, 2]);
    }

    #[test]
    fn memo_derives_value() {
        let (count, set_count) = create_signal(2i32);
        let doubled = create_memo(move || count.get() * 2);

        assert_eq!(doubled.get(), 4);
        set_count.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn memo_chains() {
        let (n, set_n) = create_signal(3i32);
        let doubled = create_memo(move || n.get() * 2);
        let quadrupled = create_memo(move || doubled.get() * 2);

        assert_eq!(quadrupled.get(), 12);
        set_n.set(5);
        assert_eq!(quadrupled.get(), 20);
    }

    #[test]
    fn batch_fires_effect_once() {
        let (a, set_a) = create_signal(0i32);
        let (b, set_b) = create_signal(0i32);
        let runs = Rc::new(RefCell::new(0usize));
        let runs_clone = Rc::clone(&runs);

        let _e = create_effect(move || {
            let _ = a.get() + b.get();
            *runs_clone.borrow_mut() += 1;
        });

        assert_eq!(*runs.borrow(), 1);

        batch(|| {
            set_a.set(1);
            set_b.set(2);
        });

        assert_eq!(*runs.borrow(), 2);
    }
}
