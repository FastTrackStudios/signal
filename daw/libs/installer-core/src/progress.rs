//! Progress event types for communicating install status to the UI.

#[derive(Debug, Clone)]
pub enum InstallEvent {
    StepStarted {
        step: InstallStep,
        label: String,
    },
    StepProgress {
        step: InstallStep,
        fraction: f32,
        message: String,
    },
    StepCompleted(InstallStep),
    StepFailed {
        step: InstallStep,
        error: String,
    },
    AllCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallStep {
    Preflight,
    DownloadReaper,
    ExtractDmg,
    CopyExtension,
    InstallSws,
    InstallReaPack,
    InstallFtsExtensions,
    DownloadLibrary,
    WriteReaperIni,
    SetupRigs,
    InstallFtsControl,
    SetupShell,
}

impl InstallStep {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Preflight => "Pre-flight checks",
            Self::DownloadReaper => "Download REAPER",
            Self::ExtractDmg => "Extract REAPER",
            Self::CopyExtension => "Install extension",
            Self::InstallSws => "Install SWS",
            Self::InstallReaPack => "Install ReaPack",
            Self::InstallFtsExtensions => "Install FTS extensions",
            Self::DownloadLibrary => "Download library",
            Self::WriteReaperIni => "Configure REAPER",
            Self::SetupRigs => "Set up rig apps",
            Self::InstallFtsControl => "Install FTS Control",
            Self::SetupShell => "Set up PATH",
        }
    }

    pub fn all() -> &'static [InstallStep] {
        &[
            Self::Preflight,
            Self::DownloadReaper,
            Self::ExtractDmg,
            Self::CopyExtension,
            Self::InstallSws,
            Self::InstallReaPack,
            Self::InstallFtsExtensions,
            Self::DownloadLibrary,
            Self::WriteReaperIni,
            Self::SetupRigs,
            Self::InstallFtsControl,
            Self::SetupShell,
        ]
    }
}

pub type EventSender = tokio::sync::mpsc::Sender<InstallEvent>;
pub type EventReceiver = tokio::sync::mpsc::Receiver<InstallEvent>;
