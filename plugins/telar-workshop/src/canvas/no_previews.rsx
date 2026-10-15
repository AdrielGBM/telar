[logic]
use telar_devtools::WORKBENCH_GRID;

use crate::strings::{self, NO_PREVIEWS, NO_PREVIEWS_HINT};

[view]
col grow:1 min_height:0 gap:WORKBENCH_GRID pad:(WORKBENCH_GRID * 2.0) align:center justify:center role:status
    text "{strings::text(NO_PREVIEWS)}" font_weight:semibold
    text "{strings::text(NO_PREVIEWS_HINT)}" max_width:480 color:$theme.muted
