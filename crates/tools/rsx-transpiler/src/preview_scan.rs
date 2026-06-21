pub struct PreviewInfo {
    pub name: String,
}

pub fn scan_previews(logic_source: &str) -> Vec<PreviewInfo> {
    let mut previews = Vec::new();
    let bytes = logic_source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if logic_source[i..].starts_with("#[preview(") {
            let start = i + "#[preview(".len();
            let mut depth = 1usize;
            let mut j = start;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    j += 1;
                }
            }
            if depth == 0 {
                let inner = &logic_source[start..j];
                if let Some(name) = extract_preview_name(inner) {
                    previews.push(PreviewInfo { name });
                }
            }
            // Skip past ')' and the following ']'.
            i = j + 2;
        } else {
            i += logic_source[i..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
        }
    }
    previews
}

fn extract_preview_name(inner: &str) -> Option<String> {
    for part in inner.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("name") {
            let rest = rest.trim().strip_prefix('=')?;
            let rest = rest.trim();
            if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                let name = &rest[1..rest.len() - 1];
                return Some(name.replace("\\\"", "\""));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_preview_attr() {
        let src = r#"let x = 1;
#[preview(name = "Default state")]
let y = 2;"#;
        let previews = scan_previews(src);
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].name, "Default state");
    }

    #[test]
    fn detects_multiple_previews() {
        let src = r#"#[preview(name = "Alpha")]
let a = 1;
#[preview(name = "Beta")]
let b = 2;"#;
        let previews = scan_previews(src);
        assert_eq!(previews.len(), 2);
        assert_eq!(previews[0].name, "Alpha");
        assert_eq!(previews[1].name, "Beta");
    }

    #[test]
    fn no_previews() {
        let src = "let x = create_rw_signal(0i32);";
        assert!(scan_previews(src).is_empty());
    }
}
