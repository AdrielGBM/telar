pub struct PreviewInfo {
    pub name: String,
}

pub fn scan_previews(logic_source: &str) -> Vec<PreviewInfo> {
    let mut previews = Vec::new();
    for line in logic_source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("#[preview(") {
            continue;
        }
        if let Some(name) = extract_preview_name(trimmed) {
            previews.push(PreviewInfo { name });
        }
    }
    previews
}

fn extract_preview_name(attr: &str) -> Option<String> {
    let inner = attr.strip_prefix("#[preview(")?.strip_suffix(")]")?;
    for part in inner.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("name") {
            let rest = rest.trim().strip_prefix('=')?;
            let rest = rest.trim();
            let name = rest.strip_prefix('"')?.strip_suffix('"')?;
            return Some(name.to_string());
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
