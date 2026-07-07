//! **Omnisphere patch import** — parse Spectrasonics `.prt_omn` patch files
//! (the "AmberPart" XML dialect) and map them onto the composition tree from
//! [`crate::omni`], realizing Soundsource blocks against the local
//! soundsource extraction by name.
//!
//! Format notes (reverse-engineered from the Settings Library):
//! - Plain XML, no declaration; root `<AmberPart><SynthEngine>…`.
//! - Numeric attributes are IEEE-754 `f32` bit patterns as 8 hex digits
//!   (`3f800000` = 1.0); small integers are written as decimal.
//! - `SYNTHENG` holds `VOICE` (one per layer: FILTER, WAVESHAPER, OSC + HARM,
//!   AENV/FENV + params, a 4-slot `EFFRACK`), a parallel `MULTISAMPLE` list
//!   (its `MS_IM_0 name= library=` names the layer's soundsource), the
//!   common `EFFRACK` + `AUXEFFRACK`, `LFO_SET` (6 LFOs), 6 `MODENV`s and
//!   the flat-attribute `MOD_MATRIX` (`source0`/`target0`/…).
//! - `ENTRYDESCR` carries the patch name, library and the browser tag string
//!   (`Author=…;Genre=…;Mood=…`).

mod xml;

pub use xml::{XmlNode, omni_num, parse_xml};
mod index;
mod model;
mod multi;
pub mod state;
mod tree;

pub use index::SoundsourceIndex;
pub use model::{OmniLayer, OmniModRoute, OmniPatch, parse_patch};
pub use multi::{OmniMulti, load_multi_file, multi_to_container, parse_multi};
pub use tree::{load_patch_file, patch_to_container};

#[cfg(test)]
use model::classify_filter_full;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const MINI_PATCH: &str = r#"<AmberPart >
<SynthEngine >
<ARP ArpOnOff="3f800000" >
</ARP>
<SYNTHENG >
<ENTRYDESCR name="Test Patch" library="Test Library" ATTRIB_VALUE_DATA="Author=Cody;Mood=Fun" >
</ENTRYDESCR>
<MOD_MATRIX source0="Layer A FENV" target0="A freq" hi0="3f000000" >
</MOD_MATRIX>
<VOICE >
<FILTER NameStr="LPF Test" act="3f800000" para="0" freq="3f000000" res="3e800000" envdpth="3f000000" envdpthinv="0" >
</FILTER>
<AENV c="3" pan="0" zoom="0" >
<p l="0" t="0" s="14" c="3f000000" >
</p>
<p l="3f800000" t="3a83126f" s="14" c="3f000000" >
</p>
<p l="3f000000" t="3c23d70a" s="14" c="3f000000" >
</p>
<p l="0" t="3ca3d70a" s="18" c="3f000000" >
</p>
</AENV>
<AENVPARAMS onOff="3f800000" attk="0" decy="0" sust="3f800000" rels="3f000000" >
</AENVPARAMS>
<FENVPARAMS onOff="3f800000" attk="3e4ccccd" decy="3f000000" sust="3f000000" rels="3f000000" >
</FENVPARAMS>
<OSC level="3f400000" fm="3e800000" am="0" hrmOn="3f800000" hrmLv="3f800000" >
<UNI umix="3f800000" ucnt="3f800000" udpth="3e4ccccd" uwdth="3f800000" >
</UNI>
<HARM Act1="3f800000" lvl1="3f000000" smi1="3f4aaaab" pan1="3f000000" wfm1="0" >
</HARM>
</OSC>
<WAVESHAPER act="3f800000" dpth="3f000000" bc="0" srrdc="0" mix="3f800000" >
</WAVESHAPER>
<EFFRACK >
<EFFMODULE Type="Chorus Echo" Active="3f800000" >
</EFFMODULE>
<EFFMODULE Type="No Effect" Active="0" >
</EFFMODULE>
</EFFRACK>
</VOICE>
<MULTISAMPLE >
<MS_IM_0 name="My Source" library="Core Library" >
</MS_IM_0>
</MULTISAMPLE>
<EFFRACK >
<EFFMODULE Type="PRO-Verb" Active="3f800000" >
</EFFMODULE>
</EFFRACK>
<AUXEFFRACK >
</AUXEFFRACK>
</SYNTHENG>
</SynthEngine>
</AmberPart>
"#;

    #[test]
    fn filter_names_classify() {
        assert_eq!(
            classify_filter_full("Classic LPF 4-pole"),
            ("lowpass", 4, "clean")
        );
        assert_eq!(
            classify_filter_full("Basic 12db Lowpass"),
            ("lowpass", 2, "clean")
        );
        assert_eq!(
            classify_filter_full("HPF Juicy 24db"),
            ("highpass", 4, "clean")
        );
        assert_eq!(
            classify_filter_full("Bandpass Juicy 12db"),
            ("bandpass", 2, "clean")
        );
        assert_eq!(classify_filter_full("Notch Filter"), ("notch", 2, "clean"));
        assert_eq!(
            classify_filter_full("Classic LPF 8-pole"),
            ("lowpass", 8, "clean")
        );
        assert_eq!(
            classify_filter_full("LPF Juicy 24db"),
            ("lowpass", 4, "ladder")
        );
        assert_eq!(
            classify_filter_full("Rich and Moogie2"),
            ("lowpass", 2, "ladder")
        );
        assert_eq!(
            classify_filter_full("Jupiter LPF 4-pole"),
            ("lowpass", 4, "ladder")
        );
        assert_eq!(classify_filter_full("untitled"), ("lowpass", 2, "clean"));
    }

    #[test]
    fn hex_floats_decode() {
        assert_eq!(omni_num("3f800000"), 1.0);
        assert_eq!(omni_num("3f000000"), 0.5);
        assert_eq!(omni_num("0"), 0.0);
        assert_eq!(omni_num("1200"), 1200.0);
    }

    #[test]
    fn parses_the_mini_patch() {
        let p = parse_patch(MINI_PATCH).unwrap();
        assert_eq!(p.name, "Test Patch");
        assert_eq!(p.library, "Test Library");
        assert_eq!(
            p.tags,
            vec![
                ("Author".to_string(), "Cody".to_string()),
                ("Mood".to_string(), "Fun".to_string())
            ]
        );
        assert!(p.arp_on);
        assert_eq!(p.layers.len(), 1);
        let l = &p.layers[0];
        assert_eq!(l.soundsource, "My Source");
        assert_eq!(l.filter_name, "LPF Test");
        assert_eq!(l.filter_freq, 0.5);
        assert_eq!(l.level, 0.75);
        assert_eq!(l.fx[0], "Chorus Echo");
        assert_eq!(p.common_fx[0], "PRO-Verb");
        // Oscillator stack: 8-voice unison at 20 cents, FM 0.25, one
        // harmonia voice +14 semitones at half level, drive-0.5 waveshaper.
        assert_eq!(l.unison_count, 8);
        assert!((l.unison_detune - 0.2).abs() < 1e-3);
        assert!((l.fm_depth - 0.25).abs() < 1e-6);
        assert_eq!(l.harmonia.len(), 1);
        let (level, smi, pan, _shape) = l.harmonia[0];
        assert!((level - 0.5).abs() < 1e-3);
        assert_eq!(smi, 14.0);
        assert!(pan.abs() < 1e-3);
        assert_eq!(l.shaper, Some((0.5, 0.0, 0.0, 1.0)));
        // Amp env comes from the AENV breakpoints (t × 100 s, calibrated
        // against the real engine): attack 0.1 s, decay 0.9 s, sustain 0.5,
        // release 1.0 s. The AENVPARAMS fallback is ignored when present.
        let (aa, ad, asus, ar) = l.amp_env.expect("amp env");
        assert!((aa - 0.1).abs() < 1e-3, "attack {aa}");
        assert!((ad - 0.9).abs() < 1e-2, "decay {ad}");
        assert!((asus - 0.5).abs() < 1e-6, "sustain {asus}");
        assert!((ar - 1.0).abs() < 1e-2, "release {ar}");
        assert!(l.filter_env.is_some());
        assert!((l.filter_env_depth - 0.5).abs() < 1e-6);
        assert_eq!(p.mod_routes.len(), 1);
        assert_eq!(p.mod_routes[0].source, "Layer A FENV");
        assert_eq!(p.mod_routes[0].target, "A freq");
    }

    #[test]
    fn maps_to_a_container() {
        let p = parse_patch(MINI_PATCH).unwrap();
        let tree = patch_to_container(&p, &SoundsourceIndex::default());
        assert_eq!(tree.name, "Test Patch");
        let layer = tree.find("Layer A").expect("layer A");
        // Unmatched soundsource stays a placeholder block with the name.
        let names: Vec<_> = layer
            .find("Oscillator")
            .unwrap()
            .blocks()
            .iter()
            .map(|b| b.display_name())
            .collect();
        assert!(names.iter().any(|n| n == "My Source"));
        // The layer FX slot carries the effect name.
        let fx: Vec<_> = layer
            .find("Layer FX")
            .unwrap()
            .blocks()
            .iter()
            .map(|b| b.display_name())
            .collect();
        assert_eq!(fx[0], "Chorus Echo");
        // Tags + mod routes survive as params.
        assert!(tree.params.iter().any(|p| p.name == "tag:Author"));
        assert!(tree.params.iter().any(|p| p.name == "mod0"));
        // Two live routes on Layer A: the filter section's own envdpth route
        // plus the matrix row — both "Filter Env" → cutoff at 0.5.
        assert_eq!(layer.mod_routes.len(), 2);
        for r in &layer.mod_routes {
            assert_eq!(r.source, "Filter Env");
            assert_eq!(r.target, "LPF Test.cutoff");
            assert!((r.depth - 0.5).abs() < 1e-6);
        }
        // The filter envelope modulator carries its imported ADSR.
        let fe = layer
            .modulators
            .iter()
            .find(|m| m.display_name() == "Filter Env")
            .expect("filter env modulator");
        assert!((fe.param_f32("sustain").unwrap() - 0.5).abs() < 1e-6);
        assert!(fe.param_f32("attack").unwrap() > 0.0);
        // The engaged filter carries the imported cutoff/res as build params.
        let f1 = layer
            .find("Filters")
            .unwrap()
            .blocks()
            .into_iter()
            .find(|b| b.display_name() == "LPF Test")
            .expect("filter 1");
        // Omnisphere freq 0.5 → 15·2^(9.55/2) ≈ 411 Hz (calibrated curve) →
        // our normalized cutoff log10(411/20)/3 ≈ 0.4375.
        assert!((f1.param_f32("cutoff").unwrap() - 0.4375).abs() < 1e-3);
        assert_eq!(f1.param_f32("resonance"), Some(0.25));
        // Renders (placeholder-safe).
        let mut rn = crate::node_render::RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 256);
        assert!(rn.live_leaves() >= 3, "native filters/amp are live");
    }

    /// Machine-local: import one of the user's own patches from the synced
    /// voyager Settings Library, realize its soundsources against the local
    /// extraction, and render it audibly.
    /// `cargo test -p signal-sampler --lib voyager_patch -- --ignored`
    #[test]
    #[ignore = "requires the voyager patch sync + soundsource extraction"]
    fn voyager_patch_imports_and_sounds() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};
        let root = Path::new(
            "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere-Voyager/Settings Library/Patches",
        );
        if !root.exists() {
            eprintln!("skipping: {root:?} not present");
            return;
        }
        let index = SoundsourceIndex::scan_default();
        assert!(!index.is_empty(), "soundsource extraction indexed");
        // First SAMPLE-mode patch whose soundsource resolves against the
        // extraction (pure synth-mode patches are placeholder-silent until
        // the Wavetable DSP lands).
        let mut patch_path = None;
        let mut stack = vec![root.to_path_buf()];
        'outer: while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            for p in entries {
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "prt_omn") {
                    let xml = std::fs::read_to_string(&p).unwrap_or_default();
                    if let Ok(parsed) = parse_patch(&xml) {
                        if parsed.layers.iter().any(|l| {
                            !l.soundsource.is_empty() && index.find(&l.soundsource).is_some()
                        }) {
                            patch_path = Some(p);
                            break 'outer;
                        }
                    }
                }
            }
        }
        let patch_path = patch_path.expect("a sample-mode .prt_omn with a matched soundsource");
        let tree = load_patch_file(&patch_path, &index).expect("import");
        eprintln!("imported {:?} as {:?}", patch_path.file_name(), tree.name);

        let mut rn = crate::node_render::RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::note_on(0, 60, 100),
        }];
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            let rms = (l.iter().map(|s| s * s).sum::<f32>() / l.len() as f32).sqrt();
            heard = heard.max(rms);
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            heard > 1e-3,
            "imported patch should be audible, rms={heard}"
        );
    }

    /// Machine-local: a pure SYNTH-mode patch (no soundsources) now sounds
    /// via the native wavetable oscillator.
    /// `cargo test -p signal-sampler --lib synth_mode_patch -- --ignored`
    #[test]
    #[ignore = "requires the voyager patch sync"]
    fn synth_mode_patch_sounds() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};
        let path = Path::new(
            "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere-Voyager/Settings Library/Patches/User/My Category/1975 Attempt.prt_omn",
        );
        if !path.exists() {
            eprintln!("skipping: {path:?} not present");
            return;
        }
        let tree = load_patch_file(path, &SoundsourceIndex::default()).expect("import");
        let mut rn = crate::node_render::RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::note_on(0, 60, 100),
        }];
        let mut heard = 0.0f32;
        for b in 0..8 {
            let ev = PluginEvents {
                params: &[],
                midi: if b == 0 { &midi } else { &[] },
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            heard = heard.max((l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt());
        }
        assert!(heard > 1e-3, "synth-mode patch audible, rms={heard}");
    }

    /// Machine-local: a user Multi from the voyager sync imports (8 parts +
    /// mixer) and renders audibly.
    /// `cargo test -p signal-sampler --lib multi_imports -- --ignored`
    #[test]
    #[ignore = "requires the voyager patch sync + soundsource extraction"]
    fn multi_imports_and_sounds() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};
        let path = Path::new(
            "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere-Voyager/Settings Library/Multis/User/Custom Sounds/1975 Fast Arp.mlt_omn",
        );
        if !path.exists() {
            eprintln!("skipping: {path:?} not present");
            return;
        }
        let index = SoundsourceIndex::scan_default();
        let tree = load_patch_file(path, &index).expect("multi import");
        eprintln!("imported multi as {:?}", tree.name);
        let mut rn = crate::node_render::RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);
        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::note_on(0, 60, 100),
        }];
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            heard = heard.max((l.iter().map(|s| s * s).sum::<f32>() / 512.0).sqrt());
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(heard > 1e-3, "multi should be audible, rms={heard}");
    }

    /// Machine-local: parse every factory patch in the on-disk Settings
    /// Library without errors (format coverage sweep).
    /// `cargo test -p signal-sampler --lib factory_patches -- --ignored`
    #[test]
    #[ignore = "requires the local Spectrasonics patch library"]
    fn factory_patches_all_parse() {
        let root = Path::new(
            "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches",
        );
        if !root.exists() {
            eprintln!("skipping: {root:?} not present");
            return;
        }
        let mut total = 0usize;
        let mut failed = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "prt_omn") {
                    total += 1;
                    let xml = std::fs::read_to_string(&p).unwrap_or_default();
                    if let Err(err) = parse_patch(&xml) {
                        failed.push(format!("{p:?}: {err}"));
                    }
                }
            }
        }
        eprintln!("parsed {total} patches, {} failures", failed.len());
        for f in failed.iter().take(10) {
            eprintln!("  {f}");
        }
        assert!(total > 1000, "expected a big factory library, got {total}");
        assert!(
            failed.is_empty(),
            "{} of {total} factory patches failed to parse",
            failed.len()
        );
    }
}
