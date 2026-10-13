//! Preferences -> Audio -> Device page child window IDs.

use crate::ChildId;

/// Preferences -> Audio -> Device page child window IDs.
pub struct DevicePrefs;

impl DevicePrefs {
    /// Audio device settings HWND - Class: #32770
    pub const AUDIO_DEVICE_SETTINGS: ChildId = ChildId(0);
    /// Audio system dropdown - Class: ComboBox
    pub const AUDIO_SYSTEM: ChildId = ChildId(1000);
    /// Audio system label - Class: Static
    pub const AUDIO_SYSTEM_LABEL: ChildId = ChildId(1001);
    /// Input device dropdown - Class: ComboBox
    pub const INPUT_DEVICE: ChildId = ChildId(1002);
    /// Output device dropdown - Class: ComboBox
    pub const OUTPUT_DEVICE: ChildId = ChildId(1003);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1004);
    /// Block size dropdown - Class: ComboBox
    pub const BLOCK_SIZE: ChildId = ChildId(1005);
    /// Request sample rate inputbox - Class: Edit
    pub const REQUEST_SAMPLE_RATE: ChildId = ChildId(1006);
    /// Request block size inputbox - Class: Edit
    pub const REQUEST_BLOCK_SIZE: ChildId = ChildId(1007);
    /// First input channel dropdown - Class: ComboBox
    pub const FIRST_INPUT_CHANNEL: ChildId = ChildId(1008);
    /// Last input channel dropdown - Class: ComboBox
    pub const LAST_INPUT_CHANNEL: ChildId = ChildId(1009);
    /// First output channel dropdown - Class: ComboBox
    pub const FIRST_OUTPUT_CHANNEL: ChildId = ChildId(1010);
    /// Last output channel dropdown - Class: ComboBox
    pub const LAST_OUTPUT_CHANNEL: ChildId = ChildId(1011);
    /// Enable inputs checkbox - Class: Button
    pub const ENABLE_INPUTS: ChildId = ChildId(1012);
    /// ASIO configuration button - Class: Button
    pub const ASIO_CONFIGURATION: ChildId = ChildId(1013);
    /// Thread priority dropdown - Class: ComboBox
    pub const THREAD_PRIORITY: ChildId = ChildId(1100);
    /// Sample rate override inputbox - Class: Edit
    pub const SAMPLE_RATE_OVERRIDE: ChildId = ChildId(1200);
    /// Latency label - Class: Static
    pub const LATENCY_LABEL: ChildId = ChildId(1300);
    /// Input channels label - Class: Static
    pub const INPUT_CHANNELS_LABEL: ChildId = ChildId(1301);
    /// Output channels label - Class: Static
    pub const OUTPUT_CHANNELS_LABEL: ChildId = ChildId(1302);
    /// Device settings label - Class: Static
    pub const DEVICE_SETTINGS_LABEL: ChildId = ChildId(1313);
}

/// Preferences -> Audio -> Device -> ASIO sub-page child window IDs.
pub struct AsioDevicePrefs;

impl AsioDevicePrefs {
    /// ASIO driver dropdown - Class: ComboBox
    pub const ASIO_DRIVER: ChildId = ChildId(1000);
    /// ASIO configuration button - Class: Button
    pub const ASIO_CONFIGURATION: ChildId = ChildId(1001);
    /// First input dropdown - Class: ComboBox
    pub const FIRST_INPUT: ChildId = ChildId(1002);
    /// Last input dropdown - Class: ComboBox
    pub const LAST_INPUT: ChildId = ChildId(1003);
    /// First output dropdown - Class: ComboBox
    pub const FIRST_OUTPUT: ChildId = ChildId(1004);
    /// Last output dropdown - Class: ComboBox
    pub const LAST_OUTPUT: ChildId = ChildId(1005);
    /// Sample rate override inputbox - Class: Edit
    pub const SAMPLE_RATE_OVERRIDE: ChildId = ChildId(1006);
    /// Thread priority dropdown - Class: ComboBox
    pub const THREAD_PRIORITY: ChildId = ChildId(1007);
    /// Block size label - Class: Static
    pub const BLOCK_SIZE_LABEL: ChildId = ChildId(1100);
}

/// Preferences -> Audio -> Device -> WASAPI sub-page child window IDs.
pub struct WasapiDevicePrefs;

impl WasapiDevicePrefs {
    /// Input device dropdown - Class: ComboBox
    pub const INPUT_DEVICE: ChildId = ChildId(1000);
    /// Output device dropdown - Class: ComboBox
    pub const OUTPUT_DEVICE: ChildId = ChildId(1001);
    /// Mode dropdown (exclusive/shared) - Class: ComboBox
    pub const MODE: ChildId = ChildId(1002);
    /// Buffer size inputbox - Class: Edit
    pub const BUFFER_SIZE: ChildId = ChildId(1003);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1004);
    /// Thread priority dropdown - Class: ComboBox
    pub const THREAD_PRIORITY: ChildId = ChildId(1005);
    /// Enable input checkbox - Class: Button
    pub const ENABLE_INPUT: ChildId = ChildId(1006);
}

/// Preferences -> Audio -> Device -> DirectSound sub-page child window IDs.
pub struct DirectSoundDevicePrefs;

impl DirectSoundDevicePrefs {
    /// Input device dropdown - Class: ComboBox
    pub const INPUT_DEVICE: ChildId = ChildId(1000);
    /// Output device dropdown - Class: ComboBox
    pub const OUTPUT_DEVICE: ChildId = ChildId(1001);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1002);
    /// Buffer size slider - Class: msctls_trackbar32
    pub const BUFFER_SIZE: ChildId = ChildId(1003);
    /// Buffer size label - Class: Static
    pub const BUFFER_SIZE_LABEL: ChildId = ChildId(1004);
    /// Enable input checkbox - Class: Button
    pub const ENABLE_INPUT: ChildId = ChildId(1005);
    /// Primary buffer checkbox - Class: Button
    pub const PRIMARY_BUFFER: ChildId = ChildId(1006);
}

/// Preferences -> Audio -> Device -> WaveOut sub-page child window IDs.
pub struct WaveOutDevicePrefs;

impl WaveOutDevicePrefs {
    /// Output device dropdown - Class: ComboBox
    pub const OUTPUT_DEVICE: ChildId = ChildId(1000);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1001);
    /// Buffer count inputbox - Class: Edit
    pub const BUFFER_COUNT: ChildId = ChildId(1002);
    /// Buffer size inputbox - Class: Edit
    pub const BUFFER_SIZE: ChildId = ChildId(1003);
}

/// Preferences -> Audio -> Device -> WDM Kernel Streaming sub-page child window IDs.
pub struct WdmDevicePrefs;

impl WdmDevicePrefs {
    /// Input device dropdown - Class: ComboBox
    pub const INPUT_DEVICE: ChildId = ChildId(1000);
    /// Output device dropdown - Class: ComboBox
    pub const OUTPUT_DEVICE: ChildId = ChildId(1001);
    /// Sample rate dropdown - Class: ComboBox
    pub const SAMPLE_RATE: ChildId = ChildId(1002);
    /// Buffer size slider - Class: msctls_trackbar32
    pub const BUFFER_SIZE: ChildId = ChildId(1003);
    /// Buffer size label - Class: Static
    pub const BUFFER_SIZE_LABEL: ChildId = ChildId(1004);
    /// Enable input checkbox - Class: Button
    pub const ENABLE_INPUT: ChildId = ChildId(1005);
}

/// Preferences -> Audio -> Device -> Dummy Audio sub-page child window IDs.
pub struct DummyDevicePrefs;

impl DummyDevicePrefs {
    /// Sample rate inputbox - Class: Edit
    pub const SAMPLE_RATE: ChildId = ChildId(1000);
    /// Block size inputbox - Class: Edit
    pub const BLOCK_SIZE: ChildId = ChildId(1001);
    /// Input channels inputbox - Class: Edit
    pub const INPUT_CHANNELS: ChildId = ChildId(1002);
    /// Output channels inputbox - Class: Edit
    pub const OUTPUT_CHANNELS: ChildId = ChildId(1003);
}
