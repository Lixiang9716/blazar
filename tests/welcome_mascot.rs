use blazar::welcome::mascot::{render_pose, MascotPose};

#[test]
fn greeting_pose_has_unicorn_cues() {
    let frame = render_pose(MascotPose::Greeting).join("\n");

    assert!(frame.contains("/|"), "horn missing");
    assert!(frame.contains("~~"), "mane missing");
    assert!(frame.contains("ᵔᴗᵔ"), "cute face missing");
    assert!(frame.contains("✦"), "sparkle missing");
}

#[test]
fn all_unicorn_poses_share_the_same_height() {
    let poses = [
        render_pose(MascotPose::Greeting),
        render_pose(MascotPose::IdleSparkle),
        render_pose(MascotPose::Listening),
    ];

    let expected = poses[0].len();
    assert!(poses.iter().all(|pose| pose.len() == expected));
}
