mod assets_layout {
    use std::path::Path;

    #[test]
    fn repository_keeps_a_tracked_assets_directory() {
        let assets_gitkeep = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/.gitkeep");
        assert!(
            assets_gitkeep.is_file(),
            "assets/.gitkeep should exist so the assets directory stays tracked"
        );
    }
}
