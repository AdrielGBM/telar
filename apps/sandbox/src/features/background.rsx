[logic]
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// Everything below the line is what actually crosses to a pool thread: plain `Send` work, no signals.
// The sleeps stand in for the blocking a real kernel does — a query, a file read, a network round trip.

fn slow_sum(n: u64) -> u64 {
    std::thread::sleep(Duration::from_millis(900));
    (1..=n).sum()
}

fn walk_project(out: Emitter<String>) {
    let names = [
        "reactive-core", "layout-core", "ui-core", "ui-tree", "ui-components",
        "renderer-core", "renderer-software", "renderer-hardware", "renderer-text",
        "platform-core", "platform-winit", "platform-desktop", "motion-core",
        "theme-core", "i18n-core", "navigate-core", "services-core", "geometry-core",
    ];
    for name in names {
        // Nothing else can stop this loop: cancelling only drops the callback, so a long worker has to
        // notice for itself. Without this check, Cancel would leave it running to the end unseen.
        if out.is_cancelled() {
            return;
        }
        std::thread::sleep(Duration::from_millis(140));
        out.emit(name.to_string());
    }
}

// The completion callbacks. They live here rather than inline in `on_press` because they must *own* their
// signals: the handler is an `Fn` and may run again, so it cannot hand its own captures away.

fn on_computed(total: RwSignal<String>, busy: RwSignal<bool>) -> impl FnOnce(u64) { move |sum| { total.set(sum.to_string()); busy.set(false); } }

fn on_found(found: RwSignal<Vec<String>>) -> impl FnMut(String) { move |name| found.update(|v| v.push(name)) }

fn on_scan_end(scanning: RwSignal<bool>) -> impl FnOnce() { move || scanning.set(false) }

// --- UI state, all of it on this thread ---

let busy = signal(false);
let total = signal(String::from("—"));

let scanning = signal(false);
let found = signal(Vec::<String>::new());
// The handle has to outlive the press that made it, or Cancel would have nothing to cancel. `Task` is
// `!Send` and not `Clone`, so it parks in an `Rc<RefCell<_>>` the two handlers share.
let scan = Rc::new(RefCell::new(None::<Task>));

// A spring the UI keeps animating *while* a worker blocks. If the work ran on this thread instead, this
// box would freeze — which is the entire reason the module exists.
let beat = motion::Animated::<f32>::new(1.0, motion::spring(170.0, 12.0));

[view]
col gap:20
    doc_header kicker:"CONCURRENCY" title:"Background work" desc:"Signals are !Send, so a worker thread can never write one. spawn_task and spawn_stream are the bridge: the work and its values cross the thread boundary, the callback stays here and runs on the UI thread."
    example title:"spawn_task — one result, delivered on the UI thread"
        card gap:12
            row gap:12 align:center
                text "sum(1..2M) · {$total}" font_size:16 color:theme.ink
                if $busy
                    spinner size:20
            row gap:10
                button label:"Compute" fill:theme.primary on_press(|| { $busy.set(true); $total.set(String::from("…")); spawn_task(|| slow_sum(2_000_000), on_computed($total.clone(), $busy.clone())); })
                button label:"Bounce" outline:theme.primary on_press(|| { $beat.retarget(if $beat.get() > 1.0 { 1.0 } else { 1.4 }) })
            row gap:16 align:center
                box fill:theme.primary radius:12 width:48 height:48 scale:$beat
                text "Press Compute, then Bounce — the spring keeps settling while the worker blocks." font_size:12 color:theme.muted
        code_line code:"spawn_task(|| slow_sum(n), move |sum| total.set(sum.to_string()))"
    example title:"spawn_stream — many values, in order, one frame after the other"
        card gap:12
            row gap:10
                button label:"Scan" fill:theme.primary on_press(|| { $found.set(Vec::new()); $scanning.set(true); *$scan.borrow_mut() = Some(spawn_stream(walk_project, on_found($found.clone()), on_scan_end($scanning.clone()))); })
                button label:"Cancel" outline:theme.danger on_press(|| { if let Some(task) = $scan.borrow_mut().take() { task.cancel(); } $scanning.set(false); })
                if $scanning
                    spinner size:20
            text "{$found.len()} crates" font_size:14 color:theme.muted
            for name in $found key name.clone()
                row gap:8 align:center pad_y:2
                    box fill:theme.success radius:4 width:6 height:6
                    text "{name}" font_size:13 color:theme.ink
        code_line code:"spawn_stream(walk_project, move |name| found.update(|v| v.push(name)))"
    example title:"The rules"
        col gap:6
            prop_row name:"work" values:"Send + 'static" about:"Runs on a pooled thread. It may block freely — the pool grows rather than starve."
            prop_row name:"callback" values:"stays here" about:"Runs on the UI thread during a later frame, so it may write signals and hold Rc."
            prop_row name:"Emitter" values:"Send, cloneable" about:"A stream's worker handle. Poll is_cancelled() in any loop that runs for a while."
            prop_row name:"Task" values:"!Send" about:"Keep it to cancel(); dropping it detaches and the callback still fires."
