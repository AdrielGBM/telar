use super::*;

fn read(entries: &[(DebugFlag, &str)]) -> DebugSettings {
    DebugSettings::read(|flag| {
        entries
            .iter()
            .find(|(set, _)| *set == flag)
            .map(|(_, value)| value.to_string())
    })
}

#[test]
fn a_run_with_nothing_set_reads_as_the_default() {
    assert_eq!(read(&[]), DebugSettings::default());
    assert_eq!(
        DebugSettings::default(),
        DebugSettings {
            perf: false,
            damage_tracking: true,
            scroll_blit: true,
        }
    );
}

#[test]
fn perf_turns_on_for_any_value_but_0() {
    assert!(read(&[(DebugFlag::Perf, "1")]).perf);
    assert!(read(&[(DebugFlag::Perf, "yes")]).perf);
    assert!(!read(&[(DebugFlag::Perf, "0")]).perf);
}

#[test]
fn damage_tracking_and_scroll_blit_turn_off_only_for_0() {
    let off = read(&[
        (DebugFlag::DamageTracking, "0"),
        (DebugFlag::ScrollBlit, "0"),
    ]);
    assert!(!off.damage_tracking);
    assert!(!off.scroll_blit);

    let on = read(&[
        (DebugFlag::DamageTracking, "1"),
        (DebugFlag::ScrollBlit, "off"),
    ]);
    assert!(on.damage_tracking);
    assert!(on.scroll_blit);
}

#[test]
fn an_empty_value_counts_as_unset() {
    let settings = read(&[
        (DebugFlag::Perf, ""),
        (DebugFlag::DamageTracking, ""),
        (DebugFlag::ScrollBlit, ""),
    ]);
    assert_eq!(settings, DebugSettings::default());
}

#[test]
fn each_flag_is_read_from_its_own_entry() {
    let settings = read(&[(DebugFlag::ScrollBlit, "0")]);
    assert_eq!(
        settings,
        DebugSettings {
            scroll_blit: false,
            ..DebugSettings::default()
        }
    );
}

#[test]
fn each_flag_has_its_own_name_in_every_source() {
    let flags = [
        DebugFlag::Perf,
        DebugFlag::DamageTracking,
        DebugFlag::ScrollBlit,
    ];
    for (i, a) in flags.iter().enumerate() {
        for b in &flags[i + 1..] {
            assert_ne!(a.env_var(), b.env_var());
            assert_ne!(a.system_property(), b.system_property());
        }
    }
}
