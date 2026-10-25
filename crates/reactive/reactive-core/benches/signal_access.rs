//! The signal hot path: creating a signal, reading and writing it with nobody watching, and reads an effect tracks.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use telar_reactive_core::{dispose_owner, effect, memo, owner_scope, signal};

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

criterion_group!(benches, create, untracked_access, tracked_access);
criterion_main!(benches);
