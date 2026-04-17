mod assets_layout {
    use std::path::Path;

    #[test]
    fn repository_keeps_a_tracked_assets_directory() {
        assert!(
            Path::new("assets/.gitkeep").is_file(),
            "assets/.gitkeep should exist so the assets directory stays tracked"
        );
    }
}
