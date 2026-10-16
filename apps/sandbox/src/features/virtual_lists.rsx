[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};

#[derive(Clone)]
struct Note {
    id: u32,
    title: String,
    body: String,
}

let notes = signal(
    (0..400u32)
        .map(|id| Note {
            id,
            title: format!("Note {id}"),
            body: "A line of text that wraps. ".repeat(1 + (id as usize * 7) % 5),
        })
        .collect::<Vec<_>>(),
);
let rows = signal(None::<VirtualRows>);

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Virtual lists" desc:"A for loop marked virtual builds only the rows its scroll shows. row_height: says every row is the same height and finds the window by division. estimated_height: lets rows differ: unbuilt rows stand at the estimate, built ones are measured, and the first row on screen holds still while estimates are corrected."
    example title:"Rows of different heights — 400 notes, a handful built"
        card gap:10
            scroll height:260 width:100%
                for note in $notes key note.id virtual estimated_height:64
                    col width:100% pad_x:12 pad_y:10 gap:4 stroke:$theme.border
                        text "{note.title}" font_size:14 font_weight:600 color:$theme.ink
                        text "{note.body}" font_size:13 color:$theme.muted
        code_line code:"for note in $notes key note.id virtual estimated_height:64"
    example title:"rows: — a handle the view can bring any row into view with"
        card gap:10
            row gap:8
                button label:"Note 40" on_press:(|| if let Some(rows) = $rows.get() { rows.reveal(40, 8.0) })
                button label:"Note 250" on_press:(|| if let Some(rows) = $rows.get() { rows.reveal(250, 8.0) })
                button label:"Note 399" variant:ButtonVariant::Outline on_press:(|| if let Some(rows) = $rows.get() { rows.reveal(399, 8.0) })
            scroll height:200 width:100%
                for note in $notes key note.id virtual estimated_height:64 overscan:96 rows:$rows
                    col width:100% pad_x:12 pad_y:10 gap:4 stroke:$theme.border
                        text "{note.title}" font_size:14 font_weight:600 color:$theme.ink
                        text "{note.body}" font_size:13 color:$theme.muted
        code_line code:"let rows = signal(None::<VirtualRows>);   for … virtual estimated_height:64 overscan:96 rows:$rows   ·   rows.reveal(index, margin)"
