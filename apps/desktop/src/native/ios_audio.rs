//! Playing on iOS as the system expects an audio app to: an audio
//! session, the lock screen's Now Playing, and the remote commands.
//!
//! - **The session** — play-and-record (the engine takes the microphone
//!   too), out of the loudspeaker rather than the earpiece, and allowed to
//!   Bluetooth (A2DP) headphones and speakers and AirPlay. Without it iOS
//!   gave a play-and-record app the earpiece and refused Bluetooth. Set as
//!   the app starts, before the engine opens the device.
//! - **Now Playing** — the song, where it is and how long it is, and whether
//!   it plays: the lock screen, Control Center, a car or headphones show it.
//! - **Remote commands** — play, pause, next and previous song from those
//!   same places. A command is queued here ([`take_commands`]) and the
//!   shell carries it out, where the transport and the set are.

use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_avf_audio::{
    AVAudioSession, AVAudioSessionCategoryOptions, AVAudioSessionCategoryPlayAndRecord,
    AVAudioSessionCategoryPlayback, AVAudioSessionModeDefault, AVAudioSessionPortOverride,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyPlaybackDuration, MPMediaItemPropertyTitle,
    MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
    MPNowPlayingInfoPropertyPlaybackRate, MPRemoteCommand, MPRemoteCommandCenter,
    MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use session_daw::device_audio::{Change, Report};

/// Where the microphone choice is kept (Settings → This device): it applies
/// as the session is set up, at launch.
fn microphone_file() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("audio-microphone"))
}

/// Whether the session takes the microphone: yes unless turned off.
fn microphone() -> bool {
    microphone_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .is_none_or(|text| text.trim() != "0")
}

/// Set the audio session up for playing a set, and make it active: with the
/// microphone, play-and-record — out of the loudspeaker rather than the
/// earpiece, Bluetooth (A2DP) and AirPlay allowed; without it, playback
/// only, which a Bluetooth device plays at its best.
pub fn configure_session() {
    // SAFETY: AVAudioSession is thread-safe; the statics are Apple's
    // constants, read after the framework has loaded.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        let mic = microphone();
        let category = if mic {
            AVAudioSessionCategoryPlayAndRecord
        } else {
            AVAudioSessionCategoryPlayback
        };
        let (Some(category), Some(mode)) = (category, AVAudioSessionModeDefault) else {
            return;
        };
        let options = if mic {
            AVAudioSessionCategoryOptions::DefaultToSpeaker
                | AVAudioSessionCategoryOptions::AllowBluetoothA2DP
                | AVAudioSessionCategoryOptions::AllowAirPlay
        } else {
            AVAudioSessionCategoryOptions::empty()
        };
        if let Err(e) = session.setCategory_mode_options_error(category, mode, options) {
            tracing::warn!(error = ?e, "ios audio: the session's category was refused");
        }
        if let Err(e) = session.setActive_error(true) {
            tracing::warn!(error = ?e, "ios audio: the session did not become active");
        }
    }
}

/// Whether the output is forced to the loudspeaker (Settings → This
/// device); the session keeps no way to ask.
static SPEAKER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The device's audio, now: its route, format and switches.
pub fn report() -> Report {
    // SAFETY: reads of the shared session.
    unsafe {
        let session = AVAudioSession::sharedInstance();
        let route = session.currentRoute();
        let names = |ports: Retained<
            objc2_foundation::NSArray<objc2_avf_audio::AVAudioSessionPortDescription>,
        >| {
            ports
                .iter()
                .map(|port| port.portName().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        Report {
            output: names(route.outputs()),
            input: names(route.inputs()),
            sample_rate: session.sampleRate(),
            buffer_ms: session.IOBufferDuration() * 1000.0,
            latency_ms: session.outputLatency() * 1000.0,
            microphone: microphone(),
            speaker: SPEAKER.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// A switch changed in Settings → This device.
pub fn change(change: Change) {
    match change {
        Change::Microphone(on) => {
            let Some(file) = microphone_file() else {
                return;
            };
            let written = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, if on { "1" } else { "0" }));
            if let Err(e) = written {
                tracing::warn!(error = %e, "ios audio: the microphone choice could not be kept");
            }
        }
        Change::Speaker(on) => {
            let port = if on {
                AVAudioSessionPortOverride::Speaker
            } else {
                AVAudioSessionPortOverride::None
            };
            // SAFETY: the shared session; an override only a
            // play-and-record session takes (refused, and said, otherwise).
            match unsafe { AVAudioSession::sharedInstance().overrideOutputAudioPort_error(port) } {
                Ok(()) => SPEAKER.store(on, std::sync::atomic::Ordering::Relaxed),
                Err(e) => {
                    tracing::warn!(error = ?e, "ios audio: the loudspeaker override was refused")
                }
            }
        }
    }
}

/// A remote command, as the shell carries it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
}

static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());

/// The remote commands since the last call, oldest first.
pub fn take_commands() -> Vec<Command> {
    COMMANDS
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

/// Listen for the remote commands: play, pause, play/pause, next and
/// previous. Once, on the main thread.
pub fn install_commands() {
    fn listen(command: &MPRemoteCommand, what: Command) {
        let handler = block2::RcBlock::new(
            move |_event: std::ptr::NonNull<MPRemoteCommandEvent>| -> MPRemoteCommandHandlerStatus {
                if let Ok(mut queue) = COMMANDS.lock() {
                    queue.push(what);
                }
                MPRemoteCommandHandlerStatus::Success
            },
        );
        // SAFETY: the command centre is used from the main thread; the
        // block is retained by the command for as long as it listens.
        unsafe {
            command.setEnabled(true);
            let _target = command.addTargetWithHandler(&handler);
        }
    }
    // SAFETY: as above.
    let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
    unsafe {
        listen(&center.playCommand(), Command::Play);
        listen(&center.pauseCommand(), Command::Pause);
        listen(&center.togglePlayPauseCommand(), Command::Toggle);
        listen(&center.nextTrackCommand(), Command::Next);
        listen(&center.previousTrackCommand(), Command::Previous);
    }
}

/// What Now Playing shows.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    /// The set it is in, shown as its album.
    pub set: String,
    /// Seconds into the song, and its length.
    pub elapsed: f64,
    pub duration: f64,
    pub playing: bool,
}

/// Show `now` as Now Playing (the lock screen, Control Center, a car).
/// iOS moves the position on by itself while the rate is 1, so this is
/// needed on a change, not every frame.
pub fn show(now: &NowPlaying) {
    fn object(o: Retained<impl objc2::Message>) -> Retained<AnyObject> {
        // SAFETY: every Objective-C object is an AnyObject.
        unsafe { Retained::cast_unchecked(o) }
    }
    // SAFETY: Apple's key constants, and an info dictionary of the types
    // MPNowPlayingInfoCenter documents for each.
    unsafe {
        let keys: [&NSString; 5] = [
            MPMediaItemPropertyTitle,
            MPMediaItemPropertyAlbumTitle,
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            MPMediaItemPropertyPlaybackDuration,
            MPNowPlayingInfoPropertyPlaybackRate,
        ];
        let values: [Retained<AnyObject>; 5] = [
            object(NSString::from_str(&now.title)),
            object(NSString::from_str(&now.set)),
            object(NSNumber::new_f64(now.elapsed.max(0.0))),
            object(NSNumber::new_f64(now.duration.max(0.0))),
            object(NSNumber::new_f64(if now.playing { 1.0 } else { 0.0 })),
        ];
        let info = NSDictionary::from_retained_objects(&keys, &values);
        MPNowPlayingInfoCenter::defaultCenter().setNowPlayingInfo(Some(&info));
    }
}

/// Nothing playing: Now Playing cleared (back at the start screen).
pub fn clear() {
    // SAFETY: the default centre, its info set to none.
    unsafe {
        MPNowPlayingInfoCenter::defaultCenter().setNowPlayingInfo(None);
    }
}
