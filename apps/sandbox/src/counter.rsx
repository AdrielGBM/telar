let count = create_rw_signal(0i32);
let double = create_memo(move || count.get() * 2);
#[preview(name = "Counter — default state")]

[style]

primary: #3d78fa
danger: #eb4444
dark: #141424
muted: #808098

.counter-card
    width: 300
    padding: 20
    gap: 12
    direction: col
    align: center

[view]

col .counter-card
    text "🚀 Counter v15" size:18 color:primary
    text "{count} / 10" size:14 color:dark
    text "Double: {double}" size:12 color:muted
    row gap:8
        btn "+" fill:primary on_press:|| count.update(|n| *n = (*n + 1).min(10))
        btn "-" outline:danger on_press:|| count.update(|n| *n = (*n - 1).max(-10))
        btn "Reset" ghost on_press:|| count.set(0)
