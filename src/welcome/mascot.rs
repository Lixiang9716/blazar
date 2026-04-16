#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MascotPose {
    OnWatch,
    TurningToUser,
    IdleMonitor,
    TypingFocus,
}

pub type Frame = &'static [&'static str];

const ON_WATCH: Frame = &[
    "          ╭────────╮        ◎",
    "          │  ◜────◝│     ◌─┼─◌",
    "          │    ╲╱  │        ◎",
    "         ╭┴────✦───┴╮",
    "        ╱  ╲      ╱  ╲",
    "       ╱____╲____╱____╲",
    "           ╱  ╲  ╱",
    "          ╱___╲╱___╲",
];

const TURNING_TO_USER: Frame = &[
    "        ◎      ╭────────╮",
    "     ◌─┼─◌     │ ◜────◝ │",
    "        ◎      │  ╲╱    │",
    "             ╭─┴──✦────┴╮",
    "            ╱   ╲    ╱   ╲",
    "           ╱____╲__╱____╲",
    "              ╱  ╲  ╱",
    "             ╱___╲╱___╲",
];

const IDLE_MONITOR: Frame = &[
    "          ╭────────╮       ◎",
    "          │ ◜────◝ │    ◌─┼─◌",
    "          │   ╲╱   │       ◎",
    "         ╭┴────✦───┴╮",
    "        ╱   ╲    ╱   ╲",
    "       ╱____╲____╱____╲",
    "           ╱  ╲  ╱",
    "          ╱___╲╱___╲",
];

const TYPING_FOCUS: Frame = &[
    "          ╭────────╮",
    "          │ ◜════◝ │      ◎",
    "          │   ╲╱   │   ◌─┼─◌",
    "         ╭┴────✦───┴╮     ◎",
    "        ╱   ╲    ╱   ╲",
    "       ╱____╲____╱____╲",
    "           ╱  ╲  ╱",
    "          ╱___╲╱___╲",
];

pub fn render_pose(pose: MascotPose) -> Frame {
    match pose {
        MascotPose::OnWatch => ON_WATCH,
        MascotPose::TurningToUser => TURNING_TO_USER,
        MascotPose::IdleMonitor => IDLE_MONITOR,
        MascotPose::TypingFocus => TYPING_FOCUS,
    }
}
