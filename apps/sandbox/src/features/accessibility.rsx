[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
use crate::shared::demo_svgs::make_icon;

let icon = make_icon();
let letters: Vec<String> = "TELAR".chars().map(String::from).collect();
let mood = signal(String::from("calm"));

[view]
col gap:20
    doc_header kicker:"ACCESSIBILITY" title:"Names, languages and hiding" desc:"label, lang and a11y:hidden say what a box is called, what language it is in, and whether a screen reader skips it. Every target reads the same words: a document writes aria-label, lang and aria-hidden, the desktop hands them to AccessKit, and a terminal writes them into its plain-text reading."
    example title:"Split letters — read as one word, not five"
        card
            row gap:4 label:"Telar" role:h2
                for letter in letters.clone()
                    text "{letter}" a11y:hidden font_size:32 font_weight:bold color:$theme.primary
        code_line code:"row label:'Telar'  ·  text '{letter}' a11y:hidden"
    example title:"A picture is decoration until it is named"
        card
            row gap:20 align:center
                svg src:icon width:40 height:40
                svg src:icon label:"Telar logo" color:$theme.primary width:40 height:40
                text "the first is skipped, the second is read as “Telar logo, image”" font_size:12 color:$theme.muted
        code_line code:"svg src:icon label:'Telar logo'"
    example title:"Language of a subtree"
        card
            col gap:6
                text "A quotation in another language keeps its own voice:" font_size:13
                text "“Caminante, no hay camino, se hace camino al andar.”" lang:"es" font_size:13 color:$theme.muted
                text "「猿も木から落ちる」" lang:"ja" font_size:13 color:$theme.muted
        code_line code:"text '…' lang:'es'"
    example title:"A name that follows state"
        card
            row gap:12 align:center
                box fill:$theme.surface_alt radius:8 pad:12 label:(format!("Mood: {}", $mood)) on_press:(|| $mood.set(if $mood.get() == "calm" { "busy".into() } else { "calm".into() }))
                    text "☺" a11y:hidden font_size:20
                text "press to change the name a reader hears" font_size:12 color:$theme.muted
        code_line code:"box label:(format!('Mood: {}', $mood))"
    example title:"Attributes"
        col gap:6
            prop_row name:"label" values:"text · t!(…) · $state expr" about:"The name assistive technology reads. On img and svg it is what makes a picture more than decoration."
            prop_row name:"lang" values:"BCP 47 tag" about:"The language of the box and everything under it; the nearest one wins."
            prop_row name:"a11y" values:"hidden" about:"Skipped by assistive technology, with its whole subtree. Still drawn and still pressable."
