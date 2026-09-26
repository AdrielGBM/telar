//! The files a browser build delivers: content-hashed names and the manifest that maps to them, the public directory copied as it is, and the precompressed copies beside both.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::media;

/// The manifest's name in the output root: a JSON object from each logical path to the hashed path that holds its bytes.
pub(crate) const MANIFEST_FILE: &str = "asset-manifest.json";

/// Hex digits of the content hash a hashed name carries: enough that two builds of one file never share a name by accident, short enough to read in a network panel.
const HASH_LEN: usize = 12;

/// The output directory as it fills, and every hashed name handed out so far.
///
/// A hashed name is a URL that can be cached forever, because a new build of the file is a new URL. A file here is written once and never overwritten: a second writer to the same path is a bug in the build, not an update.
pub(crate) struct Assets {
    root: PathBuf,
    manifest: BTreeMap<String, String>,
}

impl Assets {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            manifest: BTreeMap::new(),
        }
    }

    /// Writes `bytes` under a hashed name in the directory `logical` names, and returns the hashed path relative to the output root.
    pub(crate) fn emit(&mut self, logical: &str, bytes: &[u8]) -> Result<String, String> {
        check_logical(logical)?;
        if self.manifest.contains_key(logical) {
            return Err(format!("`{logical}` was emitted twice in one build"));
        }
        let hashed = hashed_name(logical, bytes);
        write_new(&self.root.join(&hashed), bytes)?;
        self.manifest.insert(logical.to_string(), hashed.clone());
        Ok(hashed)
    }

    /// Moves a file already in the output to its hashed name, and returns that name.
    pub(crate) fn adopt(&mut self, logical: &str) -> Result<String, String> {
        let path = self.root.join(logical);
        let bytes =
            std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let hashed = self.emit(logical, &bytes)?;
        std::fs::remove_file(&path)
            .map_err(|e| format!("could not remove {}: {e}", path.display()))?;
        Ok(hashed)
    }

    /// The manifest as it will be written.
    pub(crate) fn manifest_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(&self.manifest)
            .expect("a map of strings always serialises");
        json.push('\n');
        json
    }

    pub(crate) fn write_manifest(&self) -> Result<(), String> {
        write_new(
            &self.root.join(MANIFEST_FILE),
            self.manifest_json().as_bytes(),
        )
    }
}

/// `dir/name.ext` → `dir/name-<hash>.ext`, the hash taken over `bytes`.
pub(crate) fn hashed_name(logical: &str, bytes: &[u8]) -> String {
    let hash = &telar_project::content_hash(bytes)[..HASH_LEN];
    let (dir, file) = match logical.rsplit_once('/') {
        Some((dir, file)) => (format!("{dir}/"), file),
        None => (String::new(), logical),
    };
    match file.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{dir}{stem}-{hash}.{extension}"),
        _ => format!("{dir}{file}-{hash}"),
    }
}

fn check_logical(logical: &str) -> Result<(), String> {
    let escapes = logical.is_empty()
        || logical.starts_with('/')
        || logical.contains('\\')
        || logical
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    match escapes {
        true => Err(format!(
            "`{logical}` is not a relative path inside the output"
        )),
        false => Ok(()),
    }
}

/// Creates `path` with `bytes`, refusing to replace a file something else already put there.
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!(
                "two files of this build want {}; rename the one in the public directory",
                path.display()
            ),
            _ => format!("could not create {}: {e}", path.display()),
        })?;
    file.write_all(bytes)
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Copies every file under `from` to the same relative path under `to`, as it is: a public file is reached by a URL something outside the build already knows (`/robots.txt`, `/favicon.ico`, `/.well-known/…`), so its name cannot carry a hash. Returns how many files were copied.
///
/// `reserved` are the paths the build writes itself, refused here by name so the message can say which file to move.
pub(crate) fn copy_public(from: &Path, to: &Path, reserved: &[&str]) -> Result<usize, String> {
    let files = files_under(from)?;
    for file in &files {
        let relative = relative_url_path(from, file);
        if reserved.contains(&relative.as_str()) {
            return Err(format!(
                "{} would replace the `{relative}` the build writes itself; remove it from the public directory",
                file.display()
            ));
        }
        let bytes =
            std::fs::read(file).map_err(|e| format!("could not read {}: {e}", file.display()))?;
        write_new(&to.join(&relative), &bytes)?;
    }
    Ok(files.len())
}

/// Every file under `dir`, recursively, in a stable order.
pub(crate) fn files_under(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = std::fs::read_dir(&current)
            .map_err(|e| format!("could not read {}: {e}", current.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("could not read {}: {e}", current.display()))?
                .path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// `file` relative to `root`, with `/` between components whatever the platform uses.
fn relative_url_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Below this a compressed copy saves nothing worth a second file on disk and a second lookup at request time.
const COMPRESS_FLOOR: u64 = 1024;

/// Writes a `.br` and a `.gz` beside every file under `root` big enough, and compressible enough, to be worth one.
///
/// Compression is the server's job and stays it — but the *level* is not something a server can choose freely. Brotli at its highest takes seconds over a module this size, so nothing compresses one per request: a server runs a fast level and ships a body a hundred kilobytes larger than it had to. Doing it once here is the only way anyone ever gets the small one.
///
/// nginx (`brotli_static`, `gzip_static`), Caddy (`precompressed`) and most static hosts serve these in place of the original when the request accepts the encoding. One that has never heard of them reads the original and ignores the rest, so this costs nothing but disk.
pub(crate) fn precompress(root: &Path) -> Result<(), String> {
    for file in files_under(root)? {
        if !media::compressible(&file)
            || std::fs::metadata(&file).is_ok_and(|m| m.len() < COMPRESS_FLOOR)
        {
            continue;
        }
        let bytes =
            std::fs::read(&file).map_err(|e| format!("could not read {}: {e}", file.display()))?;
        let name = file.as_os_str().to_string_lossy();
        if let Some(encoded) = brotli(&bytes) {
            write_new(Path::new(&format!("{name}.br")), &encoded)?;
        }
        if let Some(encoded) = gzip(&bytes) {
            write_new(Path::new(&format!("{name}.gz")), &encoded)?;
        }
    }
    Ok(())
}

fn brotli(bytes: &[u8]) -> Option<Vec<u8>> {
    // Quality 11 and a 4MB window: the slow end, which is the whole reason to do this ahead of time rather than per request.
    let params = brotli::enc::BrotliEncoderParams {
        quality: 11,
        lgwin: 22,
        ..Default::default()
    };
    let mut encoded = Vec::new();
    brotli::BrotliCompress(&mut std::io::Cursor::new(bytes), &mut encoded, &params).ok()?;
    Some(encoded)
}

fn gzip(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(bytes).ok()?;
    encoder.finish().ok()
}

#[cfg(test)]
#[path = "assets_test.rs"]
mod tests;
