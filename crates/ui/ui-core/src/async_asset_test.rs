use super::*;

struct NullTransport;
impl AssetTransport for NullTransport {
    fn load(&self, _key: &AssetKey, reply: Reply) {
        reply(Err(AssetError("no transport in a test".into())));
    }
}

struct MemCache;
impl AssetCache for MemCache {
    fn get(&self, _key: &AssetKey) -> Option<Vec<u8>> {
        None
    }
    fn put(&self, _key: &AssetKey, _bytes: &[u8]) {}
}

struct UpperDecoder;
impl AssetDecoder for UpperDecoder {
    fn kind(&self) -> &'static str {
        "upper"
    }
    type Output = Arc<str>;
    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let text = std::str::from_utf8(bytes).map_err(|e| AssetError(e.to_string()))?;
        Ok(Arc::from(text.to_uppercase()))
    }
}

/// The property the trait split is built on: a loader shared across resource types needs to hold these behind `Arc<dyn _>`, and `AssetDecoder` in particular loses that if its `kind` ever goes back to being an associated const instead of a method.
#[test]
fn transport_cache_and_decoder_are_all_object_safe() {
    let transport: Arc<dyn AssetTransport> = Arc::new(NullTransport);
    let cache: Arc<dyn AssetCache> = Arc::new(MemCache);
    let decoder: Arc<dyn AssetDecoder<Output = Arc<str>>> = Arc::new(UpperDecoder);

    let key = AssetKey::new(decoder.kind(), "greeting");
    assert!(cache.get(&key).is_none(), "an empty cache holds nothing");
    cache.put(&key, b"hi");

    transport.load(
        &key,
        Box::new(|result| assert!(result.is_err(), "the stub transport answers with an error")),
    );
    assert_eq!(&*decoder.decode(b"hi").unwrap(), "HI");
}

#[test]
fn a_key_carries_the_kind_alongside_the_id_so_two_formats_cannot_collide() {
    let svg = AssetKey::new("svg", "logo");
    let image = AssetKey::new("image", "logo");
    assert_ne!(svg, image);
}

#[test]
fn a_leaked_ready_value_compares_by_identity_like_an_arc_does() {
    #[derive(Debug)]
    struct Catalog(&'static str);

    let a: &'static Catalog = Box::leak(Box::new(Catalog("catalog")));
    let b: &'static Catalog = Box::leak(Box::new(Catalog("catalog")));
    assert_eq!(AssetState::Ready(a), AssetState::Ready(a));
    assert_ne!(AssetState::Ready(a), AssetState::Ready(b));
    assert_eq!(a.0, "catalog");
    assert_eq!(
        AssetState::<&'static Catalog>::Loading,
        AssetState::<&'static Catalog>::Loading
    );
}
