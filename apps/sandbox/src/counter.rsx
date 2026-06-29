[logic]
let count = signal(0i32);
let double = memo(move || count.get() * 2);

[style]

@counter_card
    width: 300
    padding: 20
    gap: 12
    direction: col
    align: center

[view]

col @counter_card
    text "🚀 Counter" size:18 color:primary
    text "{$count} / 10" size:14 color:dark
    text "Double: {$double}" size:12 color:muted
    row gap:8
        btn "+" fill:primary on_press:|| $count.update(|n| *n = (*n + 1).min(10))
        btn "-" outline:danger on_press:|| $count.update(|n| *n = (*n - 1).max(-10))
        btn "Reset" ghost on_press:|| $count.set(0)

[preview "Counter — default state"]
counter
