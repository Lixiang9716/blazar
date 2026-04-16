use blazar::welcome::mascot::{render_pose, MascotPose};

#[test]
fn on_watch_pose_has_assistant_cues() {
    let frame = render_pose(MascotPose::OnWatch).join("\n");

    assert!(frame.contains("◜"), "soft visor missing");
    assert!(frame.contains("✦"), "chest lightcore missing");
    assert!(frame.contains("◎"), "star-map ring missing");
    assert!(frame.contains("╱"), "drape silhouette missing");
}

#[test]
fn all_mascot_poses_share_the_same_height() {
    let on_watch = render_pose(MascotPose::OnWatch);
    let typing = render_pose(MascotPose::TypingFocus);

    assert_eq!(on_watch.len(), typing.len());
}
