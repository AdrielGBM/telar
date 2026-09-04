//! The one font database, which only ever grows: loading a face is cheap and additive, where a face that vanishes from under a shaper already built from it is exactly the disagreement this module exists to prevent.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use cosmic_text::{FontSystem, fontdb};
use renderer_core::FontConfig;

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
}

impl Fonts {
    /// A `FontSystem` over these faces — a clone of the loaded database rather than a second scan, because `fontdb` holds every face as a path or a shared buffer, so what is copied is the index and not the fonts.
    pub(crate) fn font_system(&self) -> FontSystem {
        let mut db = self.db.clone();
        // Routes the default face to the first candidate that resolves. Into the copy rather than the stored database, so a configuration naming a family nothing has falls back to what a fresh load would leave rather than to whichever family the configuration before it chose.
        for name in &self.families {
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
}

static INSTALLED: RwLock<Option<Arc<Fonts>>> = RwLock::new(None);

/// Loads the faces `config` names and makes them the ones every shaper in this process uses.
///
/// Every [`TextShaper`](crate::TextShaper) calls this as it is built, so the fonts a renderer is configured with are the fonts layout measures in without anyone having to say so twice. What `config` names is *added* to the faces already loaded — a shaper built earlier keeps every face it was built from, so no later surface can pull one out from under it. Three cases, cheapest first: a config naming nothing of its own keeps what is installed exactly, because a default-configured shaper built after the application's must not throw the application's fonts away; a config whose faces are all loaded already changes only the family routed in front of them, which is what makes a family change and a surface rebuild cost no load at all; and one naming faces nobody has read yet reads those, and only those, into a copy of the database.
pub fn install(config: FontConfig) -> Arc<Fonts> {
    // Configuring the fonts a raster surface measures in says which measurer you want, so it is installed here rather than left to every runtime to remember. It yields to a frontend that installed its own.
    renderer_core::set_default_text_metrics(ShaperMetrics);
    let FontConfig {
        extra_font_paths,
        font_data,
        system_fonts_dir,
        sans_serif_family_candidates: families,
    } = config;
    let wanted = FaceSources::of(system_fonts_dir, extra_font_paths, font_data);
    if families.is_empty() && wanted == FaceSources::platform() {
        return installed();
    }
    let mut slot = INSTALLED.write().expect("font database lock");
    let Some(loaded) = slot.as_ref().cloned() else {
        let (locale, db) = wanted.load();
        return replace(
            &mut slot,
            Fonts {
                locale,
                db,
                sources: wanted,
                families,
            },
        );
    };
    let missing = loaded.sources.missing(wanted);
    if missing.is_empty() && loaded.families == families {
        return loaded;
    }
    let mut db = loaded.db.clone();
    missing.load_into(&mut db);
    let mut sources = loaded.sources.clone();
    sources.absorb(missing);
    replace(
        &mut slot,
        Fonts {
            locale: loaded.locale.clone(),
            db,
            sources,
            families,
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
    files: Vec<PathBuf>,
    // Shared with the database rather than copied into it: an embedded face is megabytes, and this outlives every shaper built from it.
    data: Vec<Arc<Vec<u8>>>,
}

impl FaceSources {
    fn of(
        system_fonts_dir: Option<PathBuf>,
        extra_font_paths: Vec<PathBuf>,
        font_data: Vec<Vec<u8>>,
    ) -> Self {
        if system_fonts_dir.is_none() && extra_font_paths.is_empty() && font_data.is_empty() {
            return Self::platform();
        }
        Self {
            system_scan: system_fonts_dir.is_none(),
            dirs: system_fonts_dir.into_iter().collect(),
            files: extra_font_paths,
            data: font_data.into_iter().map(Arc::new).collect(),
        }
    }

    /// What a shaper naming no faces of its own gets, spelled out rather than left to `fontdb`, because on one platform "the fonts this system has" is not a place `fontdb` scans.
    fn platform() -> Self {
        let mut sources = Self {
            system_scan: true,
            dirs: Vec::new(),
            files: Vec::new(),
            data: Vec::new(),
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
            files: wanted
                .files
                .into_iter()
                .filter(|file| !self.files.contains(file))
                .collect(),
            data: wanted
                .data
                .into_iter()
                .filter(|data| !self.data.contains(data))
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        !self.system_scan && self.dirs.is_empty() && self.files.is_empty() && self.data.is_empty()
    }

    fn absorb(&mut self, more: Self) {
        self.system_scan |= more.system_scan;
        self.dirs.extend(more.dirs);
        self.files.extend(more.files);
        self.data.extend(more.data);
    }

    fn load(&self) -> (String, fontdb::Database) {
        if self.system_scan && self.dirs.is_empty() && self.files.is_empty() && self.data.is_empty()
        {
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
        for file in &self.files {
            db.load_font_file(file).ok();
        }
        for data in &self.data {
            db.load_font_source(fontdb::Source::Binary(data.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test rather than four: the installed database is process-wide, so separate tests would race.
    #[test]
    fn installing_adds_to_the_faces_in_force_and_never_takes_any_away() {
        // A family nothing resolves to and faces nothing can read, so every install changes which `Fonts` is in force without changing a face — the tests measuring text beside this keep measuring the same widths.
        let named = FontConfig {
            sans_serif_family_candidates: vec!["a family no system has".to_string()],
            ..FontConfig::default()
        };
        let installed = install(named.clone());
        assert!(
            Arc::ptr_eq(&installed, &install(named.clone())),
            "the same configuration twice must load nothing and replace nothing"
        );
        assert!(
            Arc::ptr_eq(&installed, &install(FontConfig::default())),
            "a default-configured shaper built after the application's must shape in the application's fonts rather than throw them away"
        );

        let one = FontConfig {
            extra_font_paths: vec![PathBuf::from("/nonexistent/one.ttf")],
            ..named.clone()
        };
        let two = FontConfig {
            extra_font_paths: vec![PathBuf::from("/nonexistent/two.ttf")],
            ..named
        };
        install(one.clone());
        let both = install(two);
        assert!(
            Arc::ptr_eq(&both, &install(one)),
            "a shaper naming faces already read must find them, and must not cost another shaper the ones it added"
        );
    }
}
