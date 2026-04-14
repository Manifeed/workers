use std::time::{Duration, Instant};

use crate::NoticeView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
}

impl Tone {
    fn code(self) -> i32 {
        match self {
            Self::Neutral => 0,
            Self::Success => 1,
            Self::Warning => 2,
            Self::Danger => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct UiNotice {
    tone: Tone,
    text: String,
    expires_at: Option<Instant>,
}

impl UiNotice {
    pub(crate) fn neutral(text: impl Into<String>) -> Self {
        Self::transient(Tone::Neutral, text)
    }

    pub(crate) fn success(text: impl Into<String>) -> Self {
        Self::transient(Tone::Success, text)
    }

    pub(crate) fn warning(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Warning, text)
    }

    pub(crate) fn danger(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Danger, text)
    }

    pub(crate) fn neutral_persistent(text: impl Into<String>) -> Self {
        Self::persistent(Tone::Neutral, text)
    }

    fn transient(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
            expires_at: Some(Instant::now() + Duration::from_secs(5)),
        }
    }

    fn persistent(tone: Tone, text: impl Into<String>) -> Self {
        Self {
            tone,
            text: text.into(),
            expires_at: None,
        }
    }

    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        self.expires_at
            .map(|expires_at| now >= expires_at)
            .unwrap_or(false)
    }

    pub(crate) fn priority(&self) -> u8 {
        match self.tone {
            Tone::Danger => 4,
            Tone::Warning => 3,
            Tone::Neutral => 2,
            Tone::Success => 1,
        }
    }

    pub(crate) fn to_view(&self) -> NoticeView {
        NoticeView {
            visible: true,
            text: self.text.clone().into(),
            tone: self.tone.code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerStatusTone {
    Inactive,
    Active,
    Processing,
    Error,
}

impl WorkerStatusTone {
    fn code(self) -> i32 {
        match self {
            Self::Inactive => 0,
            Self::Active => 1,
            Self::Processing => 2,
            Self::Error => 3,
        }
    }
}

impl From<WorkerStatusTone> for i32 {
    fn from(value: WorkerStatusTone) -> Self {
        value.code()
    }
}

pub(crate) fn hidden_notice_view() -> NoticeView {
    NoticeView {
        visible: false,
        text: "".into(),
        tone: Tone::Neutral.code(),
    }
}
