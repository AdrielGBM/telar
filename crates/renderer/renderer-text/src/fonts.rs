//! The one font database, which only ever grows: loading a face is cheap and additive, where a face that vanishes from under a shaper already built from it is exactly the disagreement this module exists to prevent.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock, RwLockWriteGuard};

use cosmic_text::{FontSystem, fontdb};
pub use renderer_core::ExclusiveFontsError;
use renderer_core::{
    FontAsset, FontConfig, FontHost, FontSource, FontSources, FontStyle, GenericFamilies,
};

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
    /// [`Fonts::family_names`], read out of `db` the first time it is asked for.
    listed: OnceLock<Vec<String>>,
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

    /// Every family these faces answer to, sorted and each once: the name each face gives first, which is the one a declared face was registered under, leaving out the dot-prefixed faces a platform keeps for itself (`.LastResort`, `.SF NS`).
    ///
    /// Read from this database once, on first asking: the faces never change under a `Fonts`, and a face that arrives makes a new one.
    pub fn family_names(&self) -> &[String] {
        self.listed.get_or_init(|| listed_families(&self.db))
    }

    /// What these faces were loaded from and how they resolve each generic family, for [`install_sources`] to load again in another copy of this crate.
    pub fn sources(&self) -> FontSources {
        FontSources {
            system_fonts: self.sources.system_scan,
            dirs: self.sources.dirs.clone(),
            faces: self.sources.faces.clone(),
            sans_serif_family_candidates: self.families.clone(),
            generic_families: generic_families(&self.db),
        }
    }
}

fn generic_families(db: &fontdb::Database) -> GenericFamilies {
    let name = |family: fontdb::Family| db.family_name(&family).to_string();
    GenericFamilies {
        serif: name(fontdb::Family::Serif),
        sans_serif: name(fontdb::Family::SansSerif),
        monospace: name(fontdb::Family::Monospace),
        cursive: name(fontdb::Family::Cursive),
        fantasy: name(fontdb::Family::Fantasy),
    }
}

fn resolve_generic_families(db: &mut fontdb::Database, families: &GenericFamilies) {
    db.set_serif_family(families.serif.as_str());
    db.set_sans_serif_family(families.sans_serif.as_str());
    db.set_monospace_family(families.monospace.as_str());
    db.set_cursive_family(families.cursive.as_str());
    db.set_fantasy_family(families.fantasy.as_str());
}

fn listed_families(db: &fontdb::Database) -> Vec<String> {
    db.faces()
        .filter_map(|face| face.families.first())
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Every font family text can be set in here, from the database every shaper shapes in: no second scan, and no second list to disagree with the faces text resolves. Sorted, each once; see [`Fonts::family_names`].
///
/// The database only grows, so a face that arrives later ([`add_faces`], [`install_face`]) is in the answer from then on.
pub fn font_families() -> Vec<String> {
    installed().family_names().to_vec()
}

static INSTALLED: RwLock<Option<Arc<Fonts>>> = RwLock::new(None);

static HOST: RwLock<Option<Arc<dyn FontHost>>> = RwLock::new(None);

static RELEASED_ON_UNLOAD: AtomicBool = AtomicBool::new(false);

/// The installed database, for writing. A copy loaded as a guest frees it when its host unloads it, since nothing frees a static.
fn installed_slot() -> RwLockWriteGuard<'static, Option<Arc<Fonts>>> {
    release_on_unload();
    INSTALLED.write().expect("font database lock")
}

fn release_on_unload() {
    if !RELEASED_ON_UNLOAD.swap(true, Ordering::AcqRel) {
        reactive_local::thread_exit::on_unload(release_database);
    }
}

fn release_database() {
    RELEASED_ON_UNLOAD.store(false, Ordering::Release);
    let fonts = INSTALLED.write().expect("font database lock").take();
    let host = HOST.write().expect("font host lock").take();
    drop(fonts);
    drop(host);
}

fn host() -> Option<Arc<dyn FontHost>> {
    HOST.read().expect("font host lock").clone()
}

/// Makes `host` the database this copy of the crate defers to, and loads what it holds.
///
/// For a library whose text another copy of the shaper draws: a hot-reloaded app's, an embedded one's. From then on every face this copy is asked to add, by [`install`], [`add_faces`], [`install_face`] or [`install_only`], is added to `host`'s database instead, and this copy loads what `host` then holds, so the faces text is measured in here are the ones it is drawn in there. Asked for faces before anything installed any, it has `host` load its own rather than loading the platform's itself.
///
/// `host` is another copy's database, never this one's own [`Served`]: a copy deferring to itself would call itself back without end. Code this process serves itself already sets text in the one database, and is handed none.
pub fn defer_to(host: Arc<dyn FontHost>) -> Option<Arc<Fonts>> {
    release_on_unload();
    let sources = host.sources();
    *HOST.write().expect("font host lock") = Some(host);
    sources.map(install_sources)
}

/// What `host` answered with, loaded here, or what is here already when it could not answer.
fn mirror(sources: Option<FontSources>) -> Arc<Fonts> {
    sources.map_or_else(installed, install_sources)
}

/// This copy's font database, as the [`FontHost`] a library it loads defers to.
///
/// Each call catches a panic before it can leave: the library calling it links its own copy of std, and an unwind reaching its frames aborts the process.
///
/// A face the library hands over as `include_bytes!` gave it is copied before it is loaded: those bytes are in the library's image, which its host unmaps when it unloads it, and the database keeps every face for as long as the process runs.
pub struct Served;

impl FontHost for Served {
    fn sources(&self) -> Option<FontSources> {
        shielded(installed_sources).flatten()
    }

    fn install(&self, config: FontConfig) -> Option<FontSources> {
        shielded(|| {
            install(FontConfig {
                faces: held_by_host(config.faces, &installed_faces()),
                ..config
            })
            .sources()
        })
    }

    fn add_faces(&self, faces: Vec<FontAsset>) -> Option<FontSources> {
        shielded(|| add_faces(held_by_host(faces, &installed_faces())).sources())
    }

    fn install_only(
        &self,
        faces: Vec<FontAsset>,
        families: Vec<String>,
    ) -> Option<Result<FontSources, ExclusiveFontsError>> {
        shielded(|| {
            install_only(held_by_host(faces, &installed_faces()), families)
                .map(|fonts| fonts.sources())
        })
    }
}

fn installed_faces() -> Vec<FontAsset> {
    installed_sources().map_or_else(Vec::new, |sources| sources.faces)
}

/// `faces` with every static one's bytes swapped for the copy `held` already has of them, or for a copy of their own, so a face loaded once is still recognised when a reloaded library hands it over again.
fn held_by_host(faces: Vec<FontAsset>, held: &[FontAsset]) -> Vec<FontAsset> {
    faces
        .into_iter()
        .map(|mut face| {
            if let FontSource::Static(bytes) = face.source {
                face.source = held
                    .iter()
                    .map(|held| &held.source)
                    .find(|source| holds_bytes(source, bytes))
                    .cloned()
                    .unwrap_or_else(|| FontSource::Shared(Arc::from(bytes)));
            }
            face
        })
        .collect()
}

fn holds_bytes(source: &FontSource, bytes: &[u8]) -> bool {
    match source {
        FontSource::Static(held) => *held == bytes,
        FontSource::Shared(held) => **held == *bytes,
        FontSource::File(_) => false,
    }
}

fn shielded<T>(call: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
        .inspect_err(|_| tracing::error!("the font database panicked while a guest was calling it"))
        .ok()
}

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
    if let Some(host) = host() {
        return mirror(host.install(config));
    }
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
    if let Some(host) = host() {
        return mirror(host.add_faces(faces));
    }
    add(
        FaceSources {
            faces,
            ..FaceSources::none()
        },
        None,
    )
}

/// What the installed faces were loaded from and how they resolve each generic family, or `None` while nothing has installed any. Asking loads nothing.
pub fn installed_sources() -> Option<FontSources> {
    INSTALLED
        .read()
        .expect("font database lock")
        .as_ref()
        .map(|fonts| fonts.sources())
}

/// Loads the faces `sources` names and resolves each generic family as they say, so this copy of the crate sets text in exactly the faces the copy that described them does.
///
/// A hot-reloaded library links a copy of its own, whose database starts empty. Loading one itself, it lacks the application's faces and resolves the generic families as a bare platform scan does rather than as the host's configuration did, so it lays text out in a face other than the one the host draws it in. Additive like [`install`]: what is already loaded stays.
pub fn install_sources(sources: FontSources) -> Arc<Fonts> {
    renderer_core::set_default_text_metrics(ShaperMetrics);
    install_sources_into(&mut installed_slot(), &FACES_GENERATION, sources)
}

fn install_sources_into(
    slot: &mut Option<Arc<Fonts>>,
    generation: &AtomicU64,
    sources: FontSources,
) -> Arc<Fonts> {
    let FontSources {
        system_fonts,
        dirs,
        faces,
        sans_serif_family_candidates,
        generic_families: generic,
    } = sources;
    let wanted = FaceSources {
        system_scan: system_fonts,
        dirs,
        faces,
    };
    let loaded = add_into(slot, generation, wanted, Some(sans_serif_family_candidates));
    if generic_families(&loaded.db) == generic {
        return loaded;
    }
    let mut db = loaded.db.clone();
    resolve_generic_families(&mut db, &generic);
    // A generic family resolving to another face changes what a shaper already built sets text in, as a face arriving does.
    let faces = generation.fetch_add(1, Ordering::AcqRel) + 1;
    renderer_core::invalidate_text_metrics();
    replace(
        slot,
        Fonts {
            locale: loaded.locale.clone(),
            db,
            sources: loaded.sources.clone(),
            families: loaded.families.clone(),
            faces,
            listed: loaded.listed.clone(),
        },
    )
}

/// Makes `faces` the only faces any shaper in this process shapes and measures in: the platform's are never scanned, so text sets the same on every machine, and a glyph none of them has is missing rather than borrowed from whatever the machine happens to have. What a picture compared across machines needs.
///
/// The default face is routed to the first of `families` that resolves, or, when `families` is empty, to the family the first face answers to.
///
/// Fails when the process has already loaded a face `faces` does not name — the platform's, a directory's, one added since — because the database only grows and cannot be made to forget it; and when `faces` is empty, because a database with no faces has nothing to set text in.
pub fn install_only(
    faces: Vec<FontAsset>,
    families: Vec<String>,
) -> Result<Arc<Fonts>, ExclusiveFontsError> {
    renderer_core::set_default_text_metrics(ShaperMetrics);
    if let Some(host) = host() {
        return match host.install_only(faces, families) {
            Some(only) => only.map(install_sources),
            None => Err(ExclusiveFontsError::OtherFacesLoaded),
        };
    }
    install_only_into(&mut installed_slot(), &FACES_GENERATION, faces, families)
}

fn install_only_into(
    slot: &mut Option<Arc<Fonts>>,
    generation: &AtomicU64,
    faces: Vec<FontAsset>,
    families: Vec<String>,
) -> Result<Arc<Fonts>, ExclusiveFontsError> {
    if faces.is_empty() {
        return Err(ExclusiveFontsError::NoFaces);
    }
    let wanted = FaceSources {
        faces,
        ..FaceSources::none()
    };
    if let Some(loaded) = slot.as_ref()
        && !loaded.sources.within(&wanted)
    {
        return Err(ExclusiveFontsError::OtherFacesLoaded);
    }
    let fonts = add_into(
        slot,
        generation,
        wanted,
        (!families.is_empty()).then(|| families.clone()),
    );
    if !families.is_empty() {
        return Ok(fonts);
    }
    let first = fonts
        .db
        .faces()
        .find_map(|face| face.families.first())
        .map(|(name, _)| name.clone())
        .ok_or(ExclusiveFontsError::NoFaces)?;
    Ok(add_into(
        slot,
        generation,
        FaceSources::none(),
        Some(vec![first]),
    ))
}

fn add(wanted: FaceSources, families: Option<Vec<String>>) -> Arc<Fonts> {
    add_into(&mut installed_slot(), &FACES_GENERATION, wanted, families)
}

fn add_into(
    slot: &mut Option<Arc<Fonts>>,
    generation: &AtomicU64,
    wanted: FaceSources,
    families: Option<Vec<String>>,
) -> Arc<Fonts> {
    let Some(loaded) = slot.as_ref().cloned() else {
        let (locale, db) = wanted.load();
        return replace(
            slot,
            Fonts {
                locale,
                db,
                sources: wanted,
                families: families.unwrap_or_default(),
                faces: generation.load(Ordering::Acquire),
                listed: OnceLock::new(),
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
            slot,
            Fonts {
                locale: loaded.locale.clone(),
                db: loaded.db.clone(),
                sources: loaded.sources.clone(),
                families,
                faces: loaded.faces,
                listed: loaded.listed.clone(),
            },
        );
    }
    let mut db = loaded.db.clone();
    missing.load_into(&mut db);
    let mut sources = loaded.sources.clone();
    sources.absorb(missing);
    let faces = generation.fetch_add(1, Ordering::AcqRel) + 1;
    // Text measured before this may have been measured in a fallback for a family that has a face now.
    renderer_core::invalidate_text_metrics();
    replace(
        slot,
        Fonts {
            locale: loaded.locale.clone(),
            db,
            sources,
            families,
            faces,
            listed: OnceLock::new(),
        },
    )
}

/// The faces in force, loading the platform's own the first time nothing has installed any, or having the host this copy defers to load its own.
///
/// Asked on every measurement, so the common answer is one lock read: a shaper that finds the same `Arc` it was built from is still shaping in the right faces, and one that does not rebuilds itself.
pub fn installed() -> Arc<Fonts> {
    if let Some(fonts) = INSTALLED.read().expect("font database lock").as_ref() {
        return fonts.clone();
    }
    if let Some(sources) = host().and_then(|host| host.install(FontConfig::default())) {
        return install_sources(sources);
    }
    let mut slot = installed_slot();
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
            listed: OnceLock::new(),
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

    fn none() -> Self {
        Self {
            system_scan: false,
            dirs: Vec::new(),
            faces: Vec::new(),
        }
    }

    /// Whether everything these name, `other` names too.
    fn within(&self, other: &Self) -> bool {
        (!self.system_scan || other.system_scan)
            && self.dirs.iter().all(|dir| other.dirs.contains(dir))
            && self.faces.iter().all(|face| other.faces.contains(face))
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
    let (source, file) = match &asset.source {
        FontSource::Static(bytes) => (fontdb::Source::Binary(Arc::new(*bytes)), None),
        FontSource::Shared(bytes) => (fontdb::Source::Binary(Arc::new(bytes.clone())), None),
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
            (fontdb::Source::File(path.clone()), Some(path))
        }
    };
    let ids = db.load_font_source(source).to_vec();
    if ids.is_empty() {
        let name = match (&file, &asset.family) {
            (Some(path), _) => path.display().to_string(),
            (_, Some(family)) => format!("the embedded font {family:?}"),
            _ => "an embedded font".to_string(),
        };
        tracing::warn!("{name} holds no font face a shaper can read");
    }
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
/// Held for writing by the test that installs faces and for reading by a test whose shaper must not see the face generation move between two of its calls, which clears the caches it is counting.
pub(crate) static FACES_STABLE: RwLock<()> = RwLock::new(());

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
