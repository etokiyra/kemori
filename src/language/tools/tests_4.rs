#[test]
fn managed_node_asset_matches_the_pinned_version() {
    if let Some(asset) = node_asset() {
        assert!(
            asset.starts_with(&format!("node-v{NODE_VERSION}-")),
            "asset should carry the pinned version: {asset}"
        );
        assert!(
            asset.ends_with(".tar.xz") || asset.ends_with(".tar.gz") || asset.ends_with(".zip"),
            "asset should be an extractable archive: {asset}"
        );
    }
}
