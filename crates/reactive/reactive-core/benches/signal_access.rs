//! The signal hot path: creating a signal, reading and writing it with nobody watching, reads an effect tracks, and what a view pays to tell whether a value it works out read state.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use telar_reactive_core::{
    Reactive, dispose_owner, effect, memo, owner_scope, reads_state, signal,
};

fn create(c: &mut Criterion) {
    c.bench_function("signal/create_and_dispose_100", |b| {
        b.iter(|| {
            let scope = owner_scope();
            let owner = scope.id();
            for i in 0..100 {
                black_box(signal(i));
            }
            drop(scope);
            dispose_owner(owner);
        })
    });
}

fn untracked_access(c: &mut Criterion) {
    let scope = owner_scope();
    let count = signal(0u64);
    let items = signal(vec![1u64; 64]);

    c.bench_function("signal/get", |b| b.iter(|| black_box(count.get())));
    c.bench_function("signal/with", |b| {
        b.iter(|| items.with(|v| black_box(v.len())))
    });
    c.bench_function("signal/set", |b| b.iter(|| count.set(black_box(1))));
    c.bench_function("signal/update", |b| {
        b.iter(|| count.update(|v| *v = v.wrapping_add(1)))
    });
    drop(scope);
}

fn tracked_access(c: &mut Criterion) {
    let scope = owner_scope();
    let trigger = signal(0u64);
    let sources: Vec<_> = (0..16u64).map(signal).collect();
    effect(move || {
        black_box(trigger.get());
        for source in &sources {
            black_box(source.get());
        }
    });
    c.bench_function("signal/effect_rerun_reading_17", |b| {
        b.iter(|| trigger.update(|v| *v += 1))
    });

    let base = signal(1u64);
    let doubled = memo(move || base.get() * 2);
    c.bench_function("memo/get", |b| b.iter(|| black_box(doubled.get())));
    drop(scope);
}

/// What every expression a view writes now costs once where it is built: a value reading nothing is held as itself, one reading state is kept to read again, and an effect that read nothing is let go after its run.
fn probing(c: &mut Criterion) {
    let scope = owner_scope();
    let base = signal(2u64);
    c.bench_function("probe/reads_nothing", |b| {
        b.iter(|| black_box(reads_state(|| black_box(3u64) * 2)))
    });
    c.bench_function("probe/reads_a_signal", |b| {
        b.iter(|| black_box(reads_state(|| base.get() * 2)))
    });
    c.bench_function("follow/constant", |b| {
        b.iter(|| black_box(Reactive::<u64>::follow(|| black_box(3u64))))
    });
    c.bench_function("follow/reading", |b| {
        b.iter(|| black_box(Reactive::<u64>::follow(move || base.get())))
    });
    c.bench_function("effect/run_reading_nothing_and_let_go", |b| {
        b.iter(|| {
            effect(|| {
                black_box(1u64);
            })
        })
    });
    drop(scope);
}

criterion_group!(benches, create, untracked_access, tracked_access, probing);
criterion_main!(benches);
