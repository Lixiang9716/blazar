#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MascotPose {
    Greeting,
    IdleSparkle,
    Listening,
}

pub type Frame = &'static [&'static str];

const GREETING: Frame = &[
    "        ✦    ~~",
    "     /|   .-\"\"-.",
    "    /_|  ( ᵔᴗᵔ )",
    "   /  | /|   |\\",
    "  /___|/_|___|_\\",
    "      /_/   \\_\\",
];

const IDLE_SPARKLE: Frame = &[
    "      ~~    ✦ ✦",
    "     /|   .-\"\"-.",
    "    /_|  ( ᵔᴗᵔ )",
    "   /  | /|   |\\",
    "  /___|/_|___|_\\",
    "      /_/   \\_\\",
];

const LISTENING: Frame = &[
    "        ✦",
    "     /|   .-\"\"-.",
    "    /_|  ( •ᴗ• )",
    "   /  | /|   |\\",
    "  /___|/_|___|_\\",
    "      /_/   \\_\\",
];

pub fn render_pose(pose: MascotPose) -> Frame {
    match pose {
        MascotPose::Greeting => GREETING,
        MascotPose::IdleSparkle => IDLE_SPARKLE,
        MascotPose::Listening => LISTENING,
    }
}
