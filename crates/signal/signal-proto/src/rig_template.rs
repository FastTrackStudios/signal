//! # Rig Structure Templates
//!
//! Declarative descriptions of how Signal's domain hierarchy maps to REAPER tracks.
//!
//! ## Signal → REAPER Mapping
//!
//! | Signal Domain | REAPER Concept | Track Prefix | Folder? |
//! |---------------|----------------|-------------|---------|
//! | Rig           | Folder track   | `[R]`       | Yes     |
//! | Engine        | Sub-folder     | `[E]`       | Yes     |
//! | Layer         | Leaf track     | `[L]`       | No      |
//! | Module        | FX Container   | `[M]`       | N/A     |
//! | Block         | FX Plugin      | `[B]`       | N/A     |
//!
//! ## Naming Convention
//!
//! Tracks use bracket prefixes that can be toggled via `TrackDisplayOptions`:
//! - `[R] Keys Rig` — top-level rig folder
//! - `[E] Synth Engine` — engine sub-folder
//! - `[L] Synth Osc` — layer track (holds FX chain)
//!
//! FX within a layer's chain use `FxRole` prefixes:
//! - `[M] EQ: Pro-Q 4 3-Band` — module (FX container)
//! - `[B] EQ: Pro-Q 4` — standalone block (leaf FX)
//!
//! ## FX Sends Pattern
//!
//! Each engine and rig has an FX Sends sub-folder containing shared
//! effect buses (reverb, delay, etc.). Layers route to these via
//! REAPER track sends.
//!
//! ```text
//! [R] Keys Rig
//! ├── [E] Keys Engine
//! │   ├── [L] Keys Core          ──send──→ Reverb
//! │   ├── [L] Keys Space         ──send──→ Reverb, Delay
//! │   └── [FX Sends: Keys Engine]
//! │       ├── Reverb
//! │       └── Delay
//! ├── [E] Synth Engine
//! │   ├── [L] Synth Osc          ──send──→ Reverb, Delay
//! │   ├── [L] Synth Motion       ──send──→ Reverb
//! │   ├── [L] Synth Texture      ──send──→ Reverb, Delay
//! │   └── [FX Sends: Synth Engine]
//! │       ├── Reverb
//! │       └── Delay
//! └── [FX Sends: Keys Rig]
//!     ├── Reverb
//!     └── Delay
//! ```
//!
//! ## Vocal Rack Pattern
//!
//! A rack groups multiple rigs of the same type (e.g., 3 vocal channels)
//! sharing rack-level FX send groups:
//!
//! ```text
//! [Vocal Rack]
//! ├── Vocal 1 Input, Vocal 2 Input, Vocal 3 Input
//! ├── [Vocal 1 Rig]
//! │   ├── [L] Vocal 1
//! │   └── [FX Sends: Vocal 1 Rig]
//! │       ├── Verb Ambient, Verb Long
//! │       └── Delay Slap, Delay Long
//! ├── [Vocal 2 Rig] ... (same)
//! ├── [Vocal 3 Rig] ... (same)
//! └── [FX Sends: Vocal Rack]
//!     ├── [AUX] Chorus, Octave Low, Octave High, Vocoder
//!     └── [TIME] Long Verb, Short Verb, Slap, Delay
//! ```
//!
//! ## REAPER Folder Depth Convention
//!
//! Folder depth is a relative delta on each track:
//! - `+1` = start a new folder
//! - ` 0` = normal track (child of current folder)
//! - `-1` = close 1 folder level
//! - `-N` = close N folder levels
//!
//! The last track in an FX Sends section typically uses `-2` to close
//! both the sends folder and its parent engine/rig folder.

/// A complete rig's REAPER track structure.
#[derive(Debug, Clone)]
pub struct RigTemplate {
    pub name: String,
    pub engines: Vec<EngineTemplate>,
    pub fx_sends: Vec<FxSendTemplate>,
}

/// Engine = folder containing layers + FX sends.
#[derive(Debug, Clone)]
pub struct EngineTemplate {
    pub name: String,
    pub layers: Vec<LayerTemplate>,
    pub fx_sends: Vec<FxSendTemplate>,
}

/// Layer = track with modules/blocks in its FX chain.
#[derive(Debug, Clone)]
pub struct LayerTemplate {
    pub name: String,
}

/// FX send destination track.
#[derive(Debug, Clone)]
pub struct FxSendTemplate {
    pub name: String,
}

/// A rack holds multiple rigs + rack-level FX.
#[derive(Debug, Clone)]
pub struct RackTemplate {
    pub name: String,
    pub input_tracks: Vec<String>,
    pub rigs: Vec<RigTemplate>,
    pub fx_send_groups: Vec<FxSendGroupTemplate>,
}

/// Named group of send destinations (e.g. [AUX], [TIME]).
#[derive(Debug, Clone)]
pub struct FxSendGroupTemplate {
    pub name: String,
    pub sends: Vec<FxSendTemplate>,
}

// ─── Preset constructors ─────────────────────────────────────────

impl RigTemplate {
    /// Keys megarig: 3 engines (Keys, Synth, Organ) with 2-3 layers each,
    /// engine-level and rig-level FX sends.
    pub fn keys_megarig() -> Self {
        Self {
            name: "Keys Rig".into(),
            engines: vec![
                EngineTemplate {
                    name: "Keys Engine".into(),
                    layers: vec![
                        LayerTemplate {
                            name: "Keys Core".into(),
                        },
                        LayerTemplate {
                            name: "Keys Space".into(),
                        },
                    ],
                    fx_sends: vec![
                        FxSendTemplate {
                            name: "Reverb".into(),
                        },
                        FxSendTemplate {
                            name: "Delay".into(),
                        },
                    ],
                },
                EngineTemplate {
                    name: "Synth Engine".into(),
                    layers: vec![
                        LayerTemplate {
                            name: "Synth Osc".into(),
                        },
                        LayerTemplate {
                            name: "Synth Motion".into(),
                        },
                        LayerTemplate {
                            name: "Synth Texture".into(),
                        },
                    ],
                    fx_sends: vec![
                        FxSendTemplate {
                            name: "Reverb".into(),
                        },
                        FxSendTemplate {
                            name: "Delay".into(),
                        },
                    ],
                },
                EngineTemplate {
                    name: "Organ Engine".into(),
                    layers: vec![
                        LayerTemplate {
                            name: "Organ Body".into(),
                        },
                        LayerTemplate {
                            name: "Organ Air".into(),
                        },
                    ],
                    fx_sends: vec![
                        FxSendTemplate {
                            name: "Reverb".into(),
                        },
                        FxSendTemplate {
                            name: "Delay".into(),
                        },
                    ],
                },
            ],
            fx_sends: vec![
                FxSendTemplate {
                    name: "Reverb".into(),
                },
                FxSendTemplate {
                    name: "Delay".into(),
                },
            ],
        }
    }

    /// Simple guitar rig: 1 engine, 1 layer, no FX sends.
    pub fn guitar_rig() -> Self {
        Self {
            name: "Guitar Rig".into(),
            engines: vec![EngineTemplate {
                name: "Guitar Engine".into(),
                layers: vec![LayerTemplate {
                    name: "Guitar Main".into(),
                }],
                fx_sends: vec![],
            }],
            fx_sends: vec![],
        }
    }

    /// Single vocal rig channel with 4 FX sends.
    /// `index` is 1-based (Vocal 1, Vocal 2, etc.).
    pub fn vocal_rig(index: u8) -> Self {
        Self {
            name: format!("Vocal {} Rig", index),
            engines: vec![EngineTemplate {
                name: format!("Vocal {} Engine", index),
                layers: vec![LayerTemplate {
                    name: format!("Vocal {}", index),
                }],
                fx_sends: vec![
                    FxSendTemplate {
                        name: "Verb Ambient".into(),
                    },
                    FxSendTemplate {
                        name: "Verb Long".into(),
                    },
                    FxSendTemplate {
                        name: "Delay Slap".into(),
                    },
                    FxSendTemplate {
                        name: "Delay Long".into(),
                    },
                ],
            }],
            fx_sends: vec![],
        }
    }
}

impl RackTemplate {
    /// Vocal rack: 3 vocal rigs with shared AUX + TIME send groups.
    pub fn vocal_rack() -> Self {
        Self {
            name: "Vocal Rack".into(),
            input_tracks: vec![
                "Vocal 1 Input".into(),
                "Vocal 2 Input".into(),
                "Vocal 3 Input".into(),
            ],
            rigs: vec![
                RigTemplate::vocal_rig(1),
                RigTemplate::vocal_rig(2),
                RigTemplate::vocal_rig(3),
            ],
            fx_send_groups: vec![
                FxSendGroupTemplate {
                    name: "AUX".into(),
                    sends: vec![
                        FxSendTemplate {
                            name: "Chorus".into(),
                        },
                        FxSendTemplate {
                            name: "Octave Low".into(),
                        },
                        FxSendTemplate {
                            name: "Octave High".into(),
                        },
                        FxSendTemplate {
                            name: "Vocoder".into(),
                        },
                    ],
                },
                FxSendGroupTemplate {
                    name: "TIME".into(),
                    sends: vec![
                        FxSendTemplate {
                            name: "Long Verb".into(),
                        },
                        FxSendTemplate {
                            name: "Short Verb".into(),
                        },
                        FxSendTemplate {
                            name: "Slap".into(),
                        },
                        FxSendTemplate {
                            name: "Delay".into(),
                        },
                    ],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_megarig_has_correct_structure() {
        let rig = RigTemplate::keys_megarig();
        assert_eq!(rig.name, "Keys Rig");
        assert_eq!(rig.engines.len(), 3);
        assert_eq!(rig.fx_sends.len(), 2);

        // Keys engine: 2 layers
        assert_eq!(rig.engines[0].name, "Keys Engine");
        assert_eq!(rig.engines[0].layers.len(), 2);
        assert_eq!(rig.engines[0].fx_sends.len(), 2);

        // Synth engine: 3 layers
        assert_eq!(rig.engines[1].name, "Synth Engine");
        assert_eq!(rig.engines[1].layers.len(), 3);

        // Organ engine: 2 layers
        assert_eq!(rig.engines[2].name, "Organ Engine");
        assert_eq!(rig.engines[2].layers.len(), 2);
    }

    #[test]
    fn guitar_rig_is_minimal() {
        let rig = RigTemplate::guitar_rig();
        assert_eq!(rig.engines.len(), 1);
        assert_eq!(rig.engines[0].layers.len(), 1);
        assert!(rig.fx_sends.is_empty());
        assert!(rig.engines[0].fx_sends.is_empty());
    }

    #[test]
    fn vocal_rack_has_three_rigs_and_send_groups() {
        let rack = RackTemplate::vocal_rack();
        assert_eq!(rack.rigs.len(), 3);
        assert_eq!(rack.input_tracks.len(), 3);
        assert_eq!(rack.fx_send_groups.len(), 2);

        // Each vocal rig has 1 engine with 1 layer and 4 sends
        for (i, rig) in rack.rigs.iter().enumerate() {
            assert_eq!(rig.name, format!("Vocal {} Rig", i + 1));
            assert_eq!(rig.engines[0].fx_sends.len(), 4);
        }

        // AUX group has 4 sends, TIME group has 4 sends
        assert_eq!(rack.fx_send_groups[0].name, "AUX");
        assert_eq!(rack.fx_send_groups[0].sends.len(), 4);
        assert_eq!(rack.fx_send_groups[1].name, "TIME");
        assert_eq!(rack.fx_send_groups[1].sends.len(), 4);
    }

    /// Count total tracks that would be created for a rig template.
    fn count_rig_tracks(rig: &RigTemplate) -> usize {
        let mut count = 1; // rig folder track
        for engine in &rig.engines {
            count += 1; // engine folder track
            count += engine.layers.len();
            if !engine.fx_sends.is_empty() {
                count += 1; // sends folder
                count += engine.fx_sends.len();
            }
        }
        if !rig.fx_sends.is_empty() {
            count += 1; // rig sends folder
            count += rig.fx_sends.len();
        }
        count
    }

    #[test]
    fn keys_megarig_track_count() {
        let rig = RigTemplate::keys_megarig();
        // 1 rig + 3 engines + 7 layers + 3 engine send folders + 6 engine sends
        // + 1 rig send folder + 2 rig sends = 23
        assert_eq!(count_rig_tracks(&rig), 23);
    }

    #[test]
    fn guitar_rig_track_count() {
        let rig = RigTemplate::guitar_rig();
        // 1 rig + 1 engine + 1 layer = 3
        assert_eq!(count_rig_tracks(&rig), 3);
    }
}
