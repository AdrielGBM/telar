//! The one font database, which only ever grows: loading a face is cheap and additive, where a face that vanishes from under a shaper already built from it is exactly the disagreement this module exists to prevent.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use cosmic_text::{FontSystem, fontdb};
use renderer_core::{FontAsset, FontConfig, FontSource, FontStyle};

use crate::measure::ShaperMetrics;

/// The faces every shaper in this process shapes and measures in.
///
/// One database, handed out as an `Arc` and cloned into each shaper's `FontSystem`, is what makes measuring and drawing agree by construction: the same faces, the same `fontdb::ID`s, the same resolved sans-serif family. Two shapers is fine — the renderer's may live on a render thread — but two databases means layout can reserve room in one font while the frame is drawn in another, and where the platform keeps its fonts somewhere a bare scan does not look, it means no fonts at all.
pub struct Fonts {
    locale: String,
    /// As loaded, before any family is routed to `sans-serif` — see [`Fonts::font_system`].
    db: fontdb::Database,
    /// Everything every configuration so far has named, not the last one alone: what is already here is what a later one does not have to load.
    sources: FaceSources,
    families: Vec<String>,
    /// Which [`faces_generation`] this database holds.
    faces: u64,
}

impl Fonts {
    /// A `FontSystem` over these faces, routing the default face to the first of the families the configuration that installed them named.
    pub(crate) fn font_system(&self) -> FontSystem {
        self.font_system_routed(&self.families)
    }

    /// A `FontSystem` over these faces with the default face routed to the first of `families` that resolves — how a shaper built for one surface takes a face another surface loaded without taking that surface's default family too.
    ///
    /// A clone of the loaded database rather than a second scan, because `fontdb` holds every face as a path or a shared buffer, so what is copied is the index and not the fonts. Routed in the copy rather than the stored database, so a configuration naming a family nothing has falls back to what a fresh load would leave rather than to whichever family the configuration before it chose.
    pub(crate) fn font_system_routed(&self, families: &[String]) -> FontSystem {
        let mut db = self.db.clone();
        for name in families {
            if db
                .query(&fontdb::Query {
                    families: &[fontdb::Family::Name(name)],
                    ..fontdb::Query::default()
                })
                .is_some()
            {
                db.set_sans_serif_family(name.as_str());
                break;
            }
        }
        FontSystem::new_with_locale_and_db(self.locale.clone(), db)
    }

    pub(crate) fn families(&self) -> &[String] {
        &self.families
    }

    pub(crate) fn faces(&self) -> u64 {
        self.faces
    }
}

static INSTALLED: RwLock<Option<Arc<Fonts>>> = RwLock::new(None);

static FACES_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Moves every time a face is added to a database a shaper may already have been built from. A shaper compares it against the one it was built at, which is one atomic load where asking for the installed fonts is a lock.
pub(crate) fn faces_generation() -> u64 {
    FACES_GENERATION.load(Ordering::Acquire)
}

/// Loads the faces `config` names and makes them the ones every shaper in this process uses.
///
/// Every [`TextShaper`](crate::TextShaper) calls this as it is built, so the fonts a renderer is configured with are the fonts layout measures in without anyone having to say so twice. What `config` names is *added* to the faces already loaded — a shaper built earlier keeps every face it was built from, so no later surface can pull one out from under it. Three cases, cheapest first: a config naming nothing of its own keeps what is installed exactly, because a default-configured shaper built after the application's must not throw the application's fonts away; a config whose faces are all loaded already changes only the family routed in front of them, which is what makes a family change and a surface rebuild cost no load at all; and one naming faces nobody has read yet reads those, and only those, into a copy of the database.
pub fn install(config: FontConfig) -> Arc<Fonts> {
    // Configuring the fonts a raster surface measures in says which measurer you want, so it is installed here rather than left to every runtime to remember. It yields to a frontend that installed its own.
    renderer_core::set_default_text_metrics(ShaperMetrics);
    let FontConfig {
        faces,
        system_fonts_dir,
        sans_serif_family_candidates: families,
    } = config;
    let wanted = FaceSources::of(system_fonts_dir, faces);
    if families.is_empty() && wanted == FaceSources::platform() {
        return installed();
    }
    add(wanted, Some(families))
}

/// Adds `faces` to the database every shaper uses, keeping whichever family the installed configuration routes the default face to.
///
/// The seam a face that *arrives* needs — fetched by a page, downloaded, picked by the user — as opposed to one a surface is configured with. Every shaper takes it on its next use, a `Stack` naming its family re-resolves, and layout measures its text again once.
pub fn add_faces(faces: Vec<FontAsset>) -> Arc<Fonts> {
    add(
        FaceSources {
            system_scan: false,
            dirs: Vec::new(),
            faces,
        },
        None,
    )
}

fn add(wanted: FaceSources, families: Option<Vec<String>>) -> Arc<Fonts> {
    let mut slot = INSTALLED.write().expect("font database lock");
    let Some(loaded) = slot.as_ref().cloned() else {
        let (locale, db) = wanted.load();
        return replace(
            &mut slot,
            Fonts {
                locale,
                db,
                sources: wanted,
                families: families.unwrap_or_default(),
                faces: faces_generation(),
            },
        );
    };
    let families = families.unwrap_or_else(|| loaded.families.clone());
    let missing = loaded.sources.missing(wanted);
    if missing.is_empty() {
        if loaded.families == families {
            return loaded;
        }
        return replace(
            &mut slot,
            Fonts {
                locale: loaded.locale.clone(),
                db: loaded.db.clone(),
                sources: loaded.sources.clone(),
                families,
                faces: loaded.faces,
            },
        );
    }
    let mut db = loaded.db.clone();
    missing.load_into(&mut db);
    let mut sources = loaded.sources.clone();
    sources.absorb(missing);
    let faces = FACES_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    // Text measured before this may have been measured in a fallback for a family that has a face now.
    renderer_core::invalidate_text_metrics();
    replace(
        &mut slot,
        Fonts {
            locale: loaded.locale.clone(),
            db,
            sources,
            families,
            faces,
        },
    )
}

/// The faces in force, loading the platform's own the first time nothing has installed any.
///
/// Asked on every measurement, so the common answer is one lock read: a shaper that finds the same `Arc` it was built from is still shaping in the right faces, and one that does not rebuilds itself.
pub fn installed() -> Arc<Fonts> {
    if let Some(fonts) = INSTALLED.read().expect("font database lock").as_ref() {
        return fonts.clone();
    }
    let mut slot = INSTALLED.write().expect("font database lock");
    if let Some(fonts) = slot.as_ref() {
        return fonts.clone();
    }
    let sources = FaceSources::platform();
    let (locale, db) = sources.load();
    replace(
        &mut slot,
        Fonts {
            locale,
            db,
            sources,
            families: Vec::new(),
            faces: faces_generation(),
        },
    )
}

fn replace(slot: &mut Option<Arc<Fonts>>, fonts: Fonts) -> Arc<Fonts> {
    let fonts = Arc::new(fonts);
    *slot = Some(fonts.clone());
    fonts
}

// Kept beside the database, so a shaper naming faces already read clones what is loaded instead of reading them again.
#[derive(Clone, PartialEq)]
struct FaceSources {
    /// Whether the platform's own font directories have been walked. At most once in a process: it is the only one of these that costs a scan.
    system_scan: bool,
    dirs: Vec<PathBuf>,
    faces: Vec<FontAsset>,
}

impl FaceSources {
    fn of(system_fonts_dir: Option<PathBuf>, faces: Vec<FontAsset>) -> Self {
        if system_fonts_dir.is_none() && faces.is_empty() {
            return Self::platform();
        }
        Self {
            system_scan: system_fonts_dir.is_none(),
            dirs: system_fonts_dir.into_iter().collect(),
            faces,
        }
    }

    /// What a shaper naming no faces of its own gets, spelled out rather than left to `fontdb`, because on one platform "the fonts this system has" is not a place `fontdb` scans.
    fn platform() -> Self {
        let mut sources = Self {
            system_scan: true,
            dirs: Vec::new(),
            faces: Vec::new(),
        };
        if cfg!(target_os = "android") {
            // Android keeps its faces outside every directory `load_system_fonts` looks in, and a database with no faces aborts cosmic-text the first time anything is measured.
            sources.system_scan = false;
            sources.dirs.push(PathBuf::from("/system/fonts"));
        }
        sources
    }

    /// What `wanted` names and these do not, in the order `wanted` gave it — `fontdb` resolves a face by the order it was read in, so the caller's order is the caller's business.
    fn missing(&self, wanted: Self) -> Self {
        Self {
            system_scan: wanted.system_scan && !self.system_scan,
            dirs: wanted
                .dirs
                .into_iter()
                .filter(|dir| !self.dirs.contains(dir))
                .collect(),
            faces: wanted
                .faces
                .into_iter()
                .filter(|face| !self.faces.contains(face))
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        !self.system_scan && self.dirs.is_empty() && self.faces.is_empty()
    }

    fn absorb(&mut self, more: Self) {
        self.system_scan |= more.system_scan;
        self.dirs.extend(more.dirs);
        self.faces.extend(more.faces);
    }

    fn load(&self) -> (String, fontdb::Database) {
        if self.system_scan && self.dirs.is_empty() && self.faces.is_empty() {
            // cosmic-text's own default database, taken from a `FontSystem` rather than rebuilt here: a hand-rolled copy of its system scan and generic-family choices would drift from every other consumer's.
            return FontSystem::new().into_locale_and_db();
        }
        let mut db = fontdb::Database::new();
        self.load_into(&mut db);
        let locale = std::env::var("LANG").unwrap_or_else(|_| "en-US".to_string());
        (locale, db)
    }

    fn load_into(&self, db: &mut fontdb::Database) {
        if self.system_scan {
            db.load_system_fonts();
        }
        for dir in &self.dirs {
            db.load_fonts_dir(dir);
        }
        for face in &self.faces {
            load_asset(db, face);
        }
    }
}

fn load_asset(db: &mut fontdb::Database, asset: &FontAsset) -> Vec<fontdb::ID> {
    let source = match &asset.source {
        FontSource::Static(bytes) => fontdb::Source::Binary(Arc::new(*bytes)),
        FontSource::Shared(bytes) => fontdb::Source::Binary(Arc::new(bytes.clone())),
        FontSource::File(_) => {
            let executable_dir = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(PathBuf::from));
            let path = asset
                .source
                .resolved_path(executable_dir.as_deref())
                .expect("a file source resolves to a path");
            if !path.is_file() {
                tracing::warn!("font file {} does not exist", path.display());
                return Vec::new();
            }
            fontdb::Source::File(path)
        }
    };
    let ids = db.load_font_source(source).to_vec();
    let Some(family) = &asset.family else {
        return ids;
    };
    ids.into_iter()
        .filter_map(|id| {
            let mut info = db.face(id)?.clone();
            db.remove_face(id);
            declare(&mut info, asset, family);
            Some(db.push_face_info(info))
        })
        .collect()
}

/// Makes the face answer to what its declaration says, the way an `@font-face` rule does: its family first, its own names after it so nothing asking for those loses it, and the declared slant and weight as what a query matches against.
fn declare(info: &mut fontdb::FaceInfo, asset: &FontAsset, family: &str) {
    info.families.retain(|(name, _)| name != family);
    info.families.insert(
        0,
        (family.to_string(), fontdb::Language::English_UnitedStates),
    );
    info.style = match asset.style {
        FontStyle::Normal => fontdb::Style::Normal,
        FontStyle::Italic => fontdb::Style::Italic,
        FontStyle::Oblique => fontdb::Style::Oblique,
    };
    info.weight = fontdb::Weight(info.weight.0.clamp(asset.weight.min, asset.weight.max));
}

#[cfg(test)]
#[path = "fonts_test.rs"]
mod tests;

/// Loads one face from bytes and reports the families it declares, or `None` when the bytes are not a font.
///
/// A downloaded or user-supplied file has no family the caller knows until it has been read, which is what this answers; [`add_faces`] is the same load for a face whose family is already known. Loading is additive like every other path here, so a shaper already built keeps everything it had.
///
/// The families are the face's own, read out of its name table — not a name the caller chose. A file carrying several faces reports each.
pub fn install_face(data: Vec<u8>) -> Option<Vec<String>> {
    let mut probe = fontdb::Database::new();
    probe.load_font_data(data.clone());
    let families: Vec<String> = probe
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();
    if families.is_empty() {
        return None;
    }
    add_faces(vec![FontAsset::bytes(data)]);
    Some(families)
}
