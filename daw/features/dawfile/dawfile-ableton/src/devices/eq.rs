//! EQ Eight typed parameters.

use crate::parse::xml_helpers::*;
use crate::write::xml_writer::AbletonXmlWriter;
use roxmltree::Node;
use std::io::{self, Write};

/// EQ Eight parameters. Maps to REAPER's ReaEQ.
#[derive(Debug, Clone)]
pub struct Eq8Params {
    /// Oversampling: 0=high quality, 1=eco.
    pub precision: i32,
    /// 0=stereo, 1=L/R, 2=M/S.
    pub mode: i32,
    /// Overall output gain.
    pub global_gain: f64,
    /// Band gain scale percentage.
    pub scale: f64,
    /// Adaptive Q enabled.
    pub adaptive_q: bool,
    /// Adaptive Q factor.
    pub adaptive_q_factor: f64,
    /// Eight EQ bands.
    pub bands: [Eq8Band; 8],
}

/// A single EQ8 band with A/B parameter sets.
#[derive(Debug, Clone, Default)]
pub struct Eq8Band {
    pub parameter_a: Eq8BandParameter,
    pub parameter_b: Eq8BandParameter,
}

/// Parameters for one side (A or B) of an EQ8 band.
#[derive(Debug, Clone)]
pub struct Eq8BandParameter {
    /// Whether this band is active.
    pub is_on: bool,
    /// 0=LowCut48, 1=LowCut12, 2=LowShelf, 3=Bell, 4=Notch, 5=HighShelf, 6=HighCut12, 7=HighCut48.
    pub mode: i32,
    /// Frequency in Hz.
    pub freq: f64,
    /// Gain in dB.
    pub gain: f64,
    /// Resonance/Q factor.
    pub q: f64,
}

impl Default for Eq8BandParameter {
    fn default() -> Self {
        Self {
            is_on: false,
            mode: 3, // Bell
            freq: 1000.0,
            gain: 0.0,
            q: 0.71,
        }
    }
}

impl Default for Eq8Params {
    fn default() -> Self {
        Self {
            precision: 0,
            mode: 0,
            global_gain: 0.0,
            scale: 100.0,
            adaptive_q: false,
            adaptive_q_factor: 1.0,
            bands: Default::default(),
        }
    }
}

fn parse_band_parameter(node: Node<'_, '_>) -> Eq8BandParameter {
    Eq8BandParameter {
        is_on: child(node, "IsOn")
            .and_then(|n| child_bool(n, "Manual"))
            .unwrap_or(false),
        mode: child(node, "Mode")
            .and_then(|n| child_i32(n, "Manual"))
            .unwrap_or(3),
        freq: child(node, "Freq")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(1000.0),
        gain: child(node, "Gain")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(0.0),
        q: child(node, "Q")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(0.71),
    }
}

/// Parse EQ8 parameters from an Eq8 device XML node.
pub fn parse(node: Node<'_, '_>) -> Eq8Params {
    let mut params = Eq8Params {
        precision: child(node, "Precision")
            .and_then(|n| child_i32(n, "Manual"))
            .unwrap_or(0),
        mode: child(node, "Mode")
            .and_then(|n| child_i32(n, "Manual"))
            .unwrap_or(0),
        global_gain: child(node, "GlobalGain")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(0.0),
        scale: child(node, "Scale")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(100.0),
        adaptive_q: child(node, "AdaptiveQ")
            .and_then(|n| child_bool(n, "Manual"))
            .unwrap_or(false),
        adaptive_q_factor: child(node, "AdaptiveQFactor")
            .and_then(|n| child_f64(n, "Manual"))
            .unwrap_or(1.0),
        ..Default::default()
    };

    for i in 0..8 {
        let band_tag = format!("Bands.{i}");
        if let Some(band_node) = child(node, &band_tag) {
            let param_a = child(band_node, "ParameterA")
                .map(parse_band_parameter)
                .unwrap_or_default();
            let param_b = child(band_node, "ParameterB")
                .map(parse_band_parameter)
                .unwrap_or_default();
            params.bands[i] = Eq8Band {
                parameter_a: param_a,
                parameter_b: param_b,
            };
        }
    }

    params
}

fn write_band_parameter<W: Write>(
    w: &mut AbletonXmlWriter<W>,
    tag: &str,
    param: &Eq8BandParameter,
) -> io::Result<()> {
    w.start(tag)?;

    w.start("IsOn")?;
    w.value_bool("Manual", param.is_on)?;
    w.automation_target("AutomationTarget")?;
    w.end("IsOn")?;

    w.start("Mode")?;
    w.value_int("Manual", param.mode as i64)?;
    w.automation_target("AutomationTarget")?;
    w.end("Mode")?;

    w.start("Freq")?;
    w.value_float("Manual", param.freq)?;
    w.automation_target("AutomationTarget")?;
    w.end("Freq")?;

    w.start("Gain")?;
    w.value_float("Manual", param.gain)?;
    w.automation_target("AutomationTarget")?;
    w.end("Gain")?;

    w.start("Q")?;
    w.value_float("Manual", param.q)?;
    w.automation_target("AutomationTarget")?;
    w.end("Q")?;

    w.end(tag)
}

/// Write EQ8 parameters to XML.
pub fn write<W: Write>(w: &mut AbletonXmlWriter<W>, params: &Eq8Params) -> io::Result<()> {
    w.start("Precision")?;
    w.value_int("Manual", params.precision as i64)?;
    w.automation_target("AutomationTarget")?;
    w.end("Precision")?;

    w.start("Mode")?;
    w.value_int("Manual", params.mode as i64)?;
    w.automation_target("AutomationTarget")?;
    w.end("Mode")?;

    w.start("GlobalGain")?;
    w.value_float("Manual", params.global_gain)?;
    w.automation_target("AutomationTarget")?;
    w.end("GlobalGain")?;

    w.start("Scale")?;
    w.value_float("Manual", params.scale)?;
    w.automation_target("AutomationTarget")?;
    w.end("Scale")?;

    w.start("AdaptiveQ")?;
    w.value_bool("Manual", params.adaptive_q)?;
    w.end("AdaptiveQ")?;

    w.start("AdaptiveQFactor")?;
    w.value_float("Manual", params.adaptive_q_factor)?;
    w.end("AdaptiveQFactor")?;

    for (i, band) in params.bands.iter().enumerate() {
        let band_tag = format!("Bands.{i}");
        w.start(&band_tag)?;
        write_band_parameter(w, "ParameterA", &band.parameter_a)?;
        write_band_parameter(w, "ParameterB", &band.parameter_b)?;
        w.end(&band_tag)?;
    }

    Ok(())
}
