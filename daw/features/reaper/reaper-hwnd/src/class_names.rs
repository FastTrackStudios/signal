//! Window class name constants and enum for REAPER's HWND windows.
//!
//! Compiled from Meo-Ada Mespotine's documentation of REAPER window classes.

use serde::{Deserialize, Serialize};

// ── String constants ──

/// Reaper's MainHwnd class
pub const REAPER_WND: &str = "REAPERwnd";
/// Virtual MIDI keyboard
pub const REAPER_VKB_CTL: &str = "REAPERvkbctl";
/// ArrangeView
pub const REAPER_TRACK_LIST_WINDOW: &str = "REAPERTrackListWindow";
/// Several dialog elements
pub const REAPER_VIRT_WND_DLG_HOST: &str = "REAPERVirtWndDlgHost";
/// Ruler
pub const REAPER_TIME_DISPLAY: &str = "REAPERTimeDisplay";
/// MasterVU in Mixer
pub const REAPER_VU: &str = "REAPERvu";
/// VU of a track in trackview
pub const REAPER_TRACK_VU: &str = "REAPERtrackvu";
/// Unknown
pub const REAPER_HORZ_VU: &str = "REAPERhorzvu";
/// Unknown
pub const REAPER_VERT_VU: &str = "REAPERvertvu";
/// VU of a track in the mixer
pub const REAPER_MIXER_VU: &str = "REAPERmixervu";
/// Timer display in Transport
pub const REAPER_STATUS_DISP: &str = "REAPERstatusdisp";
/// Video window class
pub const REAPER_VIDEO_MAIN_WND: &str = "REAPERVideoMainwnd";
/// Editor area of IDE
pub const WDL_CURSES_WINDOW: &str = "WDLCursesWindow";
/// ReaEQ eq field
pub const REA_EQ_WINDOW: &str = "ReaEQWindow";
/// Vertical fader in dialog
pub const REAPER_V_FADER: &str = "REAPERvfader";
/// Horizontal fader in dialog
pub const REAPER_H_FADER: &str = "REAPERhfader";
/// Presetlist
pub const COMBO_BOX: &str = "ComboBox";
/// Dropdown of presetlist
pub const COMBO_LBOX: &str = "ComboLBox";
/// ReaFir plugin editor
pub const FFT_DISP: &str = "FFTdisp";
/// Editable inputbox
pub const EDIT: &str = "Edit";
/// Button/checkbox/radio
pub const BUTTON: &str = "Button";
/// Richtext editor area
pub const RICH_EDIT_CHILD: &str = "RichEditChild";
/// Themed buttons (ReaNINJAM)
pub const HOT_TRACK_BUTTON: &str = "HotTrackButton";
/// Shown text
pub const STATIC: &str = "Static";
/// ReaSurround editing field
pub const REA_SURROUND_WINDOW: &str = "ReaSurroundWindow";
/// Knob in dialog
pub const REAPER_KNOB: &str = "REAPERknob";
/// ReaTune display
pub const REA_TUNE_CTRL: &str = "ReatuneCtrl";
/// ReaVerb impulse window
pub const IMP_DISP: &str = "ImpDisp";
/// ListView (tables/lists)
pub const SYS_LIST_VIEW_32: &str = "SysListView32";
/// ReaXcomp editing field
pub const REA_XCOMP_WINDOW: &str = "ReaXcompWindow";
/// ReaSamplOmatic5000 editing area
pub const SPL_DISP: &str = "SplDisp";
/// Parent HWND for other HWNDs
pub const DIALOG_PARENT: &str = "#32770";
/// Project tabs
pub const WDL_TAB_CTRL: &str = "WDLTabCtrl";
/// Slider
pub const MSCTLS_TRACKBAR_32: &str = "msctls_trackbar32";
/// Tabs
pub const SYS_TAB_CONTROL_32: &str = "SysTabControl32";
/// Peaks display in Media Explorer
pub const ME_PEAKS: &str = "MEpeaks";
/// Media Explorer main window
pub const REAPER_MEDIA_EXPLORER_MAIN_WND: &str = "REAPERMediaExplorerMainwnd";
/// Mini peaks display
pub const REAPER_MINI_PEAKS: &str = "REAPERminipeaks";
/// Horizontal pan fader
pub const REAPER_H_PAN_FADER: &str = "REAPERhpanfader";
/// Docker container window (hosts docked panels)
pub const REAPER_DOCK: &str = "REAPER_dock";
/// Track Control Panel display (arrange-view track list)
pub const REAPER_TCP_DISPLAY: &str = "REAPERTCPDisplay";
/// Mixer Control Panel display (mixer track list)
pub const REAPER_MCP_DISPLAY: &str = "REAPERMCPDisplay";

// ── Enum ──

/// Typed enum of all known REAPER window class names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WindowClass {
    /// Reaper's MainHwnd class (`REAPERwnd`)
    ReaperWnd,
    /// Virtual MIDI keyboard (`REAPERvkbctl`)
    ReaperVkbCtl,
    /// ArrangeView (`REAPERTrackListWindow`)
    ReaperTrackListWindow,
    /// Several dialog elements (`REAPERVirtWndDlgHost`)
    ReaperVirtWndDlgHost,
    /// Ruler (`REAPERTimeDisplay`)
    ReaperTimeDisplay,
    /// MasterVU in Mixer (`REAPERvu`)
    ReaperVu,
    /// VU of a track in trackview (`REAPERtrackvu`)
    ReaperTrackVu,
    /// Unknown (`REAPERhorzvu`)
    ReaperHorzVu,
    /// Unknown (`REAPERvertvu`)
    ReaperVertVu,
    /// VU of a track in the mixer (`REAPERmixervu`)
    ReaperMixerVu,
    /// Timer display in Transport (`REAPERstatusdisp`)
    ReaperStatusDisp,
    /// Video window class (`REAPERVideoMainwnd`)
    ReaperVideoMainWnd,
    /// Editor area of IDE (`WDLCursesWindow`)
    WdlCursesWindow,
    /// ReaEQ eq field (`ReaEQWindow`)
    ReaEqWindow,
    /// Vertical fader in dialog (`REAPERvfader`)
    ReaperVFader,
    /// Horizontal fader in dialog (`REAPERhfader`)
    ReaperHFader,
    /// Presetlist (`ComboBox`)
    ComboBox,
    /// Dropdown of presetlist (`ComboLBox`)
    ComboLBox,
    /// ReaFir plugin editor (`FFTdisp`)
    FftDisp,
    /// Editable inputbox (`Edit`)
    Edit,
    /// Button/checkbox/radio (`Button`)
    Button,
    /// Richtext editor area (`RichEditChild`)
    RichEditChild,
    /// Themed buttons, ReaNINJAM (`HotTrackButton`)
    HotTrackButton,
    /// Shown text (`Static`)
    Static,
    /// ReaSurround editing field (`ReaSurroundWindow`)
    ReaSurroundWindow,
    /// Knob in dialog (`REAPERknob`)
    ReaperKnob,
    /// ReaTune display (`ReatuneCtrl`)
    ReaTuneCtrl,
    /// ReaVerb impulse window (`ImpDisp`)
    ImpDisp,
    /// ListView / tables / lists (`SysListView32`)
    SysListView32,
    /// ReaXcomp editing field (`ReaXcompWindow`)
    ReaXcompWindow,
    /// ReaSamplOmatic5000 editing area (`SplDisp`)
    SplDisp,
    /// Parent HWND for other HWNDs (`#32770`)
    DialogParent,
    /// Project tabs (`WDLTabCtrl`)
    WdlTabCtrl,
    /// Slider (`msctls_trackbar32`)
    MsctlsTrackbar32,
    /// Tabs (`SysTabControl32`)
    SysTabControl32,
    /// Peaks display in Media Explorer (`MEpeaks`)
    MePeaks,
    /// Media Explorer main window (`REAPERMediaExplorerMainwnd`)
    ReaperMediaExplorerMainWnd,
    /// Mini peaks display (`REAPERminipeaks`)
    ReaperMiniPeaks,
    /// Horizontal pan fader (`REAPERhpanfader`)
    ReaperHPanFader,
    /// Docker container window (`REAPER_dock`)
    ReaperDock,
    /// Track Control Panel display (`REAPERTCPDisplay`)
    ReaperTcpDisplay,
    /// Mixer Control Panel display (`REAPERMCPDisplay`)
    ReaperMcpDisplay,
}

impl WindowClass {
    /// Returns the raw class name string as used by the Windows API / REAPER.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReaperWnd => REAPER_WND,
            Self::ReaperVkbCtl => REAPER_VKB_CTL,
            Self::ReaperTrackListWindow => REAPER_TRACK_LIST_WINDOW,
            Self::ReaperVirtWndDlgHost => REAPER_VIRT_WND_DLG_HOST,
            Self::ReaperTimeDisplay => REAPER_TIME_DISPLAY,
            Self::ReaperVu => REAPER_VU,
            Self::ReaperTrackVu => REAPER_TRACK_VU,
            Self::ReaperHorzVu => REAPER_HORZ_VU,
            Self::ReaperVertVu => REAPER_VERT_VU,
            Self::ReaperMixerVu => REAPER_MIXER_VU,
            Self::ReaperStatusDisp => REAPER_STATUS_DISP,
            Self::ReaperVideoMainWnd => REAPER_VIDEO_MAIN_WND,
            Self::WdlCursesWindow => WDL_CURSES_WINDOW,
            Self::ReaEqWindow => REA_EQ_WINDOW,
            Self::ReaperVFader => REAPER_V_FADER,
            Self::ReaperHFader => REAPER_H_FADER,
            Self::ComboBox => COMBO_BOX,
            Self::ComboLBox => COMBO_LBOX,
            Self::FftDisp => FFT_DISP,
            Self::Edit => EDIT,
            Self::Button => BUTTON,
            Self::RichEditChild => RICH_EDIT_CHILD,
            Self::HotTrackButton => HOT_TRACK_BUTTON,
            Self::Static => STATIC,
            Self::ReaSurroundWindow => REA_SURROUND_WINDOW,
            Self::ReaperKnob => REAPER_KNOB,
            Self::ReaTuneCtrl => REA_TUNE_CTRL,
            Self::ImpDisp => IMP_DISP,
            Self::SysListView32 => SYS_LIST_VIEW_32,
            Self::ReaXcompWindow => REA_XCOMP_WINDOW,
            Self::SplDisp => SPL_DISP,
            Self::DialogParent => DIALOG_PARENT,
            Self::WdlTabCtrl => WDL_TAB_CTRL,
            Self::MsctlsTrackbar32 => MSCTLS_TRACKBAR_32,
            Self::SysTabControl32 => SYS_TAB_CONTROL_32,
            Self::MePeaks => ME_PEAKS,
            Self::ReaperMediaExplorerMainWnd => REAPER_MEDIA_EXPLORER_MAIN_WND,
            Self::ReaperMiniPeaks => REAPER_MINI_PEAKS,
            Self::ReaperHPanFader => REAPER_H_PAN_FADER,
            Self::ReaperDock => REAPER_DOCK,
            Self::ReaperTcpDisplay => REAPER_TCP_DISPLAY,
            Self::ReaperMcpDisplay => REAPER_MCP_DISPLAY,
        }
    }

    /// Parses a raw class name string into a `WindowClass` variant.
    pub fn from_class_str(s: &str) -> Option<Self> {
        match s {
            REAPER_WND => Some(Self::ReaperWnd),
            REAPER_VKB_CTL => Some(Self::ReaperVkbCtl),
            REAPER_TRACK_LIST_WINDOW => Some(Self::ReaperTrackListWindow),
            REAPER_VIRT_WND_DLG_HOST => Some(Self::ReaperVirtWndDlgHost),
            REAPER_TIME_DISPLAY => Some(Self::ReaperTimeDisplay),
            REAPER_VU => Some(Self::ReaperVu),
            REAPER_TRACK_VU => Some(Self::ReaperTrackVu),
            REAPER_HORZ_VU => Some(Self::ReaperHorzVu),
            REAPER_VERT_VU => Some(Self::ReaperVertVu),
            REAPER_MIXER_VU => Some(Self::ReaperMixerVu),
            REAPER_STATUS_DISP => Some(Self::ReaperStatusDisp),
            REAPER_VIDEO_MAIN_WND => Some(Self::ReaperVideoMainWnd),
            WDL_CURSES_WINDOW => Some(Self::WdlCursesWindow),
            REA_EQ_WINDOW => Some(Self::ReaEqWindow),
            REAPER_V_FADER => Some(Self::ReaperVFader),
            REAPER_H_FADER => Some(Self::ReaperHFader),
            COMBO_BOX => Some(Self::ComboBox),
            COMBO_LBOX => Some(Self::ComboLBox),
            FFT_DISP => Some(Self::FftDisp),
            EDIT => Some(Self::Edit),
            BUTTON => Some(Self::Button),
            RICH_EDIT_CHILD => Some(Self::RichEditChild),
            HOT_TRACK_BUTTON => Some(Self::HotTrackButton),
            STATIC => Some(Self::Static),
            REA_SURROUND_WINDOW => Some(Self::ReaSurroundWindow),
            REAPER_KNOB => Some(Self::ReaperKnob),
            REA_TUNE_CTRL => Some(Self::ReaTuneCtrl),
            IMP_DISP => Some(Self::ImpDisp),
            SYS_LIST_VIEW_32 => Some(Self::SysListView32),
            REA_XCOMP_WINDOW => Some(Self::ReaXcompWindow),
            SPL_DISP => Some(Self::SplDisp),
            DIALOG_PARENT => Some(Self::DialogParent),
            WDL_TAB_CTRL => Some(Self::WdlTabCtrl),
            MSCTLS_TRACKBAR_32 => Some(Self::MsctlsTrackbar32),
            SYS_TAB_CONTROL_32 => Some(Self::SysTabControl32),
            ME_PEAKS => Some(Self::MePeaks),
            REAPER_MEDIA_EXPLORER_MAIN_WND => Some(Self::ReaperMediaExplorerMainWnd),
            REAPER_MINI_PEAKS => Some(Self::ReaperMiniPeaks),
            REAPER_H_PAN_FADER => Some(Self::ReaperHPanFader),
            REAPER_DOCK => Some(Self::ReaperDock),
            REAPER_TCP_DISPLAY => Some(Self::ReaperTcpDisplay),
            REAPER_MCP_DISPLAY => Some(Self::ReaperMcpDisplay),
            _ => None,
        }
    }

    /// Returns all known window class variants.
    pub fn all() -> &'static [WindowClass] {
        &[
            Self::ReaperWnd,
            Self::ReaperVkbCtl,
            Self::ReaperTrackListWindow,
            Self::ReaperVirtWndDlgHost,
            Self::ReaperTimeDisplay,
            Self::ReaperVu,
            Self::ReaperTrackVu,
            Self::ReaperHorzVu,
            Self::ReaperVertVu,
            Self::ReaperMixerVu,
            Self::ReaperStatusDisp,
            Self::ReaperVideoMainWnd,
            Self::WdlCursesWindow,
            Self::ReaEqWindow,
            Self::ReaperVFader,
            Self::ReaperHFader,
            Self::ComboBox,
            Self::ComboLBox,
            Self::FftDisp,
            Self::Edit,
            Self::Button,
            Self::RichEditChild,
            Self::HotTrackButton,
            Self::Static,
            Self::ReaSurroundWindow,
            Self::ReaperKnob,
            Self::ReaTuneCtrl,
            Self::ImpDisp,
            Self::SysListView32,
            Self::ReaXcompWindow,
            Self::SplDisp,
            Self::DialogParent,
            Self::WdlTabCtrl,
            Self::MsctlsTrackbar32,
            Self::SysTabControl32,
            Self::MePeaks,
            Self::ReaperMediaExplorerMainWnd,
            Self::ReaperMiniPeaks,
            Self::ReaperHPanFader,
        ]
    }
}

impl core::fmt::Display for WindowClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
