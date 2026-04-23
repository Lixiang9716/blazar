#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusVisual {
    RunningDot,
    EndedDot,
    ErrorX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResultMode {
    Markdown,
    Code,
    Diff,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryDescriptor {
    pub status_visual: StatusVisual,
    pub title: String,
    pub subtitle: Option<String>,
    pub preview_lines: Vec<String>,
    pub result_mode: ResultMode,
    pub call_identity: Option<String>,
}
