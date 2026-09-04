//! Shadow pre-passes: rendering a blurred shape once and caching it against its geometry.

/// Text shadows draw glyph instances, path shadows draw indexed vertices.
pub(super) enum ShadowKind {
    Text {
        instance_start: u32,
        instance_end: u32,
    },
    Path {
        index_start: u32,
        index_end: u32,
    },
}

pub(super) struct ShadowOp {
    pub(super) kind: ShadowKind,
    pub(super) sigma: f32,
    pub(super) texture_width: u32,
    pub(super) texture_height: u32,
    pub(super) dest: [f32; 4],
}

// Mirrors `ShadowKind`: text keys on instance range plus instance hash, path on index range plus geometry hash.
#[derive(Hash, PartialEq, Eq, Clone)]
pub(super) enum ShadowCacheKind {
    Text {
        instance_start: u32,
        instance_count: u32,
        instances_hash: u64,
    },
    Path {
        index_start: u32,
        index_count: u32,
        geometry_hash: u64,
    },
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub(crate) struct ShadowCacheKey {
    pub(super) kind: ShadowCacheKind,
    pub(super) sigma_bits: u32,
    pub(super) texture_width: u32,
    pub(super) texture_height: u32,
}
