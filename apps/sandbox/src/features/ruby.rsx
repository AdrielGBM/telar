[logic]
use crate::shared::components::card::{CardProps, card};
use crate::shared::components::code_line::{CodeLineProps, code_line};
use crate::shared::components::doc_header::{DocHeaderProps, doc_header};
use crate::shared::components::example::{ExampleProps, example};
use crate::shared::components::prop_row::{PropRowProps, prop_row};

let shown = signal(true);
let reading = move || shown.get().then(|| String::from("よ"));

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Ruby" desc:"ruby sets a reading above a run of a paragraph, as furigana sits over kanji: half the size of its base and centred over it, the line growing to hold it. The base never breaks across lines, a reading longer than its base shrinks and closes up before it widens it, and a reader hears the base and then its reading."
    example title:"Furigana over kanji"
        card gap:10
            text font_size:24 color:$theme.ink
                span "漢字" ruby:"かんじ"
                span "を"
                span "読" ruby:"よ"
                span "む。"
            text font_size:24 color:$theme.ink
                span "東京" ruby:"とうきょう"
                span "へ"
                span "承" ruby:"うけたまわ"
                span "る。"
        code_line code:"text   span \"漢字\" ruby:\"かんじ\"   span \"を\""
    example title:"A reading that comes and goes, with a run that is styled and links"
        card gap:10
            text font_size:20 color:$theme.ink
                span "本" ruby:"ほん" color:$theme.primary font_weight:700
                span "を"
                span "読む" ruby:(reading())
                span "。"
            box on_press:(move || shown.set(!shown.get())) cursor:pointer
                text "Show or hide the reading of 読む" font_size:14 color:$theme.primary underline
            text font_size:16 color:$theme.ink
                span "A pronunciation guide: "
                span "colonel" ruby:"KER-nl" to:external("https://en.wiktionary.org/wiki/colonel")
        code_line code:"span \"読む\" ruby:(shown.get().then(|| String::from(\"よ\")))"
    example title:"On each target"
        col gap:8
            text "Desktop, Android, the web canvas and headless set the reading with the glyph shaper, over the base's em box; the web document writes <ruby> with <rt>, and <rp> parentheses for a browser that sets none; the terminal writes the reading after its base in parentheses, 漢字(かんじ), since a cell has no room above a letter." font_size:13 color:$theme.muted
    example title:"Attributes"
        col gap:6
            prop_row name:"ruby" values:"\"text\" · String · Option<String>" about:"On a span: the reading set above it. None sets none, for a reading shown only sometimes."
