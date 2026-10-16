//! A copy of the shaper that defers to a host's font database: what a hot-reloaded or embedded library's copy does, so every face it adds lands in the database its frames are drawn from.

use std::sync::{Arc, Mutex, PoisonError};

use renderer_core::{
    ExclusiveFontsError, FontAsset, FontConfig, FontHost, FontSources, GenericFamilies,
};
use telar_renderer_text::fonts;

const TEST_FACE: &[u8] = include_bytes!("../test-fonts/TelarTest.ttf");

/// A host's database, as the guest sees it: what it holds, and what it was asked.
#[derive(Default)]
struct Host {
    held: Mutex<Option<FontSources>>,
    asked: Mutex<Vec<&'static str>>,
}

impl Host {
    fn ask(&self, what: &'static str) {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(what);
    }

    fn asked(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.asked.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn load(&self, faces: Vec<FontAsset>) -> Option<FontSources> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let held = held.get_or_insert_with(nothing);
        held.faces.extend(faces);
        Some(held.clone())
    }

    fn faces(&self) -> usize {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(0, |held| held.faces.len())
    }
}

fn nothing() -> FontSources {
    FontSources {
        system_fonts: false,
        dirs: Vec::new(),
        faces: Vec::new(),
        sans_serif_family_candidates: Vec::new(),
        generic_families: GenericFamilies {
            serif: "Arrived".to_owned(),
            sans_serif: "Arrived".to_owned(),
            monospace: "Arrived".to_owned(),
            cursive: "Arrived".to_owned(),
            fantasy: "Arrived".to_owned(),
        },
    }
}

impl FontHost for Host {
    fn sources(&self) -> Option<FontSources> {
        self.ask("sources");
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn install(&self, config: FontConfig) -> Option<FontSources> {
        self.ask("install");
        self.load(config.faces)
    }

    fn add_faces(&self, faces: Vec<FontAsset>) -> Option<FontSources> {
        self.ask("add_faces");
        self.load(faces)
    }

    fn install_only(
        &self,
        _faces: Vec<FontAsset>,
        _families: Vec<String>,
    ) -> Option<Result<FontSources, ExclusiveFontsError>> {
        self.ask("install_only");
        Some(Err(ExclusiveFontsError::OtherFacesLoaded))
    }
}

fn listed(family: &str) -> bool {
    fonts::installed()
        .family_names()
        .iter()
        .any(|name| name == family)
}

/// Each case is a step of one sequence: the copy defers for the rest of the process once it is told to.
#[test]
fn every_face_the_guest_adds_reaches_the_host_before_the_guest_loads_it() {
    let host = Arc::new(Host::default());

    assert!(
        fonts::defer_to(host.clone()).is_none(),
        "a host that has loaded nothing hands nothing over"
    );
    assert_eq!(host.asked(), ["sources"]);

    fonts::installed();
    assert_eq!(
        host.asked(),
        ["install"],
        "asked for faces before any were installed, the guest has the host load its own rather than scanning the platform itself"
    );

    fonts::add_faces(vec![FontAsset::embedded(TEST_FACE).named("Arrived")]);
    assert_eq!(host.asked(), ["add_faces"]);
    assert_eq!(host.faces(), 1, "the host holds the face that arrived");
    assert!(listed("Arrived"), "and so does the guest, from the host");

    let families = fonts::install_face(TEST_FACE.to_vec()).expect("the bytes are a font");
    assert!(!families.is_empty());
    assert_eq!(host.asked(), ["add_faces"]);
    assert_eq!(host.faces(), 2);

    fonts::install(FontConfig {
        faces: vec![FontAsset::embedded(TEST_FACE).named("Configured")],
        ..FontConfig::default()
    });
    assert_eq!(host.asked(), ["install"]);
    assert!(listed("Configured"));

    assert_eq!(
        fonts::install_only(vec![FontAsset::embedded(TEST_FACE)], Vec::new()).err(),
        Some(ExclusiveFontsError::OtherFacesLoaded),
        "the host decides whether its faces can be made the only ones"
    );
    assert_eq!(host.asked(), ["install_only"]);
}
