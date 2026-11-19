# Ruby

A reading set above the run of a paragraph it reads: furigana over kanji, or a pronunciation guide over a word.
The run is the reading's base. Both stay one paragraph, shaped, wrapped and measured as one text, so a reading
never moves a line break the paragraph would not have made itself.

```rsx
text font_size:24
    span "漢字" ruby:"かんじ"
    span "を"
    span "読む" ruby:$reading          // a String, or an Option<String> for a reading shown only sometimes
    span "。"
```

From Rust, a run takes its reading with `TextRun::ruby`, and a span scanned from a string with `Span::with_ruby`.
An application holding a base string and its readings as byte ranges writes the second:

```rust
let spans = move || {
    readings
        .iter()
        .map(|(range, reading)| {
            Span::new(range.start as u32..range.end as u32, Declared::default()).with_ruby(reading.as_str())
        })
        .collect()
};
Text::spanned_declaring(move || base.clone(), spans, LayoutStyle::new(), |style| style)?
```

A span's other properties apply as they always do: a reading takes the ink of the run it reads, a run with a
reading can still be bold, coloured or a link, and a press anywhere over the base or its reading lands on the run.

## How a reading is set

| | |
| --- | --- |
| Size | Half the size of its base, `RubyStyle::size`. |
| Place | Centred over its base, on the base's ideographic em box: 0.88 of the base's size above its baseline. |
| Line | A line holding a reading grows by whatever of the reading the leading above it cannot hold, in whole pixels; a line without one, or one whose `line_height` already leaves room, keeps its height. |
| Base | Never broken across lines: its grapheme clusters are glued with word joiners before the paragraph is broken, so it moves to the next line whole. |
| Long reading | One wider than its base first shrinks, down to `RubyStyle::min_size` (0.46 of the base's size); then closes its letters up, each by at most a tenth of its size (`RubyStyle::TIGHTEST`); and only what is left widens the base, its letters spread evenly around each one. |
| Reader | Hears the base and then its reading, `漢字(かんじ)`, in the paragraph and in the name of a link it sits on. |

`RubyStyle { size, min_size }` is a field of `TextStyle`, set with `.with_ruby(…)` in the style a text is given or
amends (`Text::runs(runs, layout, |style| style.with_ruby(RubyStyle { size: 0.4, min_size: 0.36 }))`). A reading
is shown as written: the case a run is shown in does not reach it.

A reading that comes and goes measures the paragraph again: the line that held it gives its room back.

## On each target

| Target | Ruby |
| --- | --- |
| Desktop, Android, web canvas, headless (GPU and software) | Fitted, placed and drawn by the glyph shaper, the readings with the rest of the paragraph's glyphs; measure, draw and hit-testing read the same lines. |
| web-dom | `<ruby>` with `white-space: nowrap`, the reading an `<rt>` at the size and tightening the fit gave it with `line-height: 1`, between `<rp>` parentheses for a browser that sets no ruby. The browser places the reading on the face's content area rather than its em box and grows the line itself; the canvas measurer takes the same room. A page written ahead of time sets every reading at its own size until the browser takes it over. |
| TUI | Written after its base in parentheses, `漢字(かんじ)`, inside the base's run, since a cell has no room above a letter: no line grows, the cells measured are the cells drawn, and a press on the reading lands on its base. |

## Limits

- Horizontal text only: there is no vertical writing mode, and no ruby beside a vertical base.
- A base wider than its whole line breaks where it must, as any word wider than its line does, and its reading
  is drawn over the part on the first line.
- A clamp that cuts into a base drops the base's reading.
- The size is set per paragraph, not inherited from a container.
