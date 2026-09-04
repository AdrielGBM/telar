//! Vector artwork generated in code, for the SVG sections to draw.

use std::sync::Arc;
use telar::SvgData;

/// A simple stroked icon, for the SVG sections to draw.
pub fn make_icon() -> Arc<SvgData> {
    let src = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
<path d="M12 2l2.9 6.6 7.1.6-5.4 4.7 1.6 7-6.2-3.8-6.2 3.8 1.6-7-5.4-4.7 7.1-.6z" fill="#333333"/>
</svg>"##;
    Arc::new(SvgData::from_str(src).expect("valid icon svg"))
}

/// A multi-path mark, exercising fills and groups.
pub fn make_logo() -> Arc<SvgData> {
    let src = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs>
<linearGradient id="g1" x1="0" y1="0" x2="1" y2="1">
<stop offset="0" stop-color="#ff6b6b"/>
<stop offset="1" stop-color="#4ecdc4"/>
</linearGradient>
</defs>
<path d="M4 26a22 22 0 1 0 44 0a22 22 0 1 0 -44 0Z" fill="url(#g1)"/>
<path d="M24 40a16 16 0 1 0 32 0a16 16 0 1 0 -32 0Z" fill="#ffe66d" fill-opacity="0.85"/>
<path d="M12 50L32 14L52 50Z" fill="#1a535c" fill-opacity="0.55"/>
</svg>"##;
    Arc::new(SvgData::from_str(src).expect("valid logo svg"))
}

/// Artwork with a filter, which forces the raster fallback.
pub fn make_blurred() -> Arc<SvgData> {
    let src = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs>
<filter id="b" x="-50%" y="-50%" width="200%" height="200%">
<feGaussianBlur in="SourceGraphic" stdDeviation="4"/>
</filter>
</defs>
<circle cx="32" cy="32" r="20" fill="#e63946" filter="url(#b)"/>
</svg>"##;
    Arc::new(SvgData::from_str(src).expect("valid blurred svg"))
}
