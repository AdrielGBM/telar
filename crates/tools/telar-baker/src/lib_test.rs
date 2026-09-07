use super::*;

#[test]
fn every_asset_kind_has_exactly_one_baker() {
    for kind in telar_transpiler::ASSET_KINDS {
        let matches = bakers().iter().filter(|b| b.kind().id == kind.id).count();
        assert_eq!(matches, 1, "asset kind `{}` has {matches} bakers", kind.id);
    }
}
