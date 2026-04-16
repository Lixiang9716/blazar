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
    let poses = [
        render_pose(MascotPose::OnWatch),
        render_pose(MascotPose::TurningToUser),
        render_pose(MascotPose::IdleMonitor),
        render_pose(MascotPose::TypingFocus),
    ];

    let expected_height = poses[0].len();
    for (i, pose) in poses.iter().enumerate() {
        assert_eq!(pose.len(), expected_height, "pose at index {} has unexpected height", i);
    }
}
