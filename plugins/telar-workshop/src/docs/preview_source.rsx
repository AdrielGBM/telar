[logic]
use telar::preview::PreviewEntry;
use telar_components::{CodeViewProps, code_view};

use crate::strings::{self, SOURCE_OF};

/// A preview's source as it is written, numbered from the line it starts on where that is known.
pub struct Props {
    pub entry: PreviewEntry,
}

let name = props.entry.name;
let first_line = props.entry.source_spans.first().map(|span| span.line);

[view]
code_view code:props.entry.source line_numbers:(first_line.is_some()) first_line:(first_line.unwrap_or(1)) label:(Reactive::of(move || strings::text_with(SOURCE_OF, name)))
