//! Round-trip tests: write a project, read it back, assert key fields match.

use dawfile_dawproject::{
    Application, Arrangement, AudioContent, AutomationPoint, AutomationPoints, AutomationTarget,
    BuiltinDeviceContent, Channel, ChannelRole, Clip, ClipContent, CompressorParams, ContentType,
    DawProject, Device, DeviceFormat, EqBand, EqBandType, ExpressionType, FileReference,
    Interpolation, Lane, LaneContent, Marker, NoiseGateParams, Note, ProjectMetadata, Scene, Send,
    TempoPoint, TimeSignaturePoint, TimeUnit, Track, Transport, Warps, feature_support,
};

fn minimal_project() -> DawProject {
    DawProject {
        version: "1.0".to_string(),
        application: Some(Application {
            name: "FastTrackStudio".to_string(),
            version: "0.1.0".to_string(),
        }),
        metadata: Some(ProjectMetadata {
            title: Some("Test Session".to_string()),
            artist: Some("Test Artist".to_string()),
            ..Default::default()
        }),
        transport: Transport {
            tempo: 132.5,
            numerator: 3,
            denominator: 8,
        },
        tracks: vec![
            Track {
                id: "track-1".to_string(),
                name: "Bass".to_string(),
                color: Some("#FF4400".to_string()),
                comment: Some("low end".to_string()),
                content_types: vec![ContentType::Notes],
                loaded: true,
                channel: Some(Channel {
                    id: "ch-1".to_string(),
                    role: ChannelRole::Regular,
                    audio_channels: 2,
                    destination: Some("ch-master".to_string()),
                    blend_mode: None,
                    volume: 0.8,
                    pan: -0.25,
                    muted: false,
                    solo: true,
                    sends: vec![Send {
                        destination: "ch-master".to_string(),
                        volume: 1.0,
                        pan: 0.0,
                        enabled: true,
                        pre_fader: false,
                    }],
                    devices: vec![],
                }),
                children: vec![],
            },
            Track {
                id: "track-master".to_string(),
                name: "Master".to_string(),
                color: None,
                comment: None,
                content_types: vec![ContentType::Audio],
                loaded: true,
                channel: Some(Channel {
                    id: "ch-master".to_string(),
                    role: ChannelRole::Master,
                    audio_channels: 2,
                    destination: None,
                    blend_mode: None,
                    volume: 1.0,
                    pan: 0.0,
                    muted: false,
                    solo: false,
                    sends: vec![],
                    devices: vec![],
                }),
                children: vec![],
            },
        ],
        arrangement: Some(Arrangement {
            id: "arr-1".to_string(),
            name: Some("Main Arrangement".to_string()),
            color: None,
            comment: None,
            time_unit: TimeUnit::Beats,
            markers: vec![],
            lanes: vec![
                Lane {
                    id: "lane-1".to_string(),
                    track: "track-1".to_string(),
                    time_unit: None,
                    content: LaneContent::Clips(vec![Clip {
                        id: "clip-1".to_string(),
                        time: 0.0,
                        duration: 4.0,
                        time_unit: None,
                        content_time_unit: Some(TimeUnit::Beats),
                        name: Some("Intro".to_string()),
                        color: None,
                        comment: Some("first clip".to_string()),
                        enabled: true,
                        play_start: Some(0.5),
                        play_stop: Some(3.5),
                        reference: None,
                        fade_in_time: Some(0.25),
                        fade_out_time: Some(0.5),
                        fade_time_unit: Some(TimeUnit::Beats),
                        loop_settings: None,
                        content: ClipContent::Notes(vec![
                            Note {
                                time: 0.0,
                                duration: 0.5,
                                channel: 0,
                                key: 40,
                                velocity: 0.8,
                                release_velocity: None,
                            },
                            Note {
                                time: 1.0,
                                duration: 0.5,
                                channel: 0,
                                key: 43,
                                velocity: 0.7,
                                release_velocity: Some(0.5),
                            },
                        ]),
                    }]),
                },
                Lane {
                    id: "lane-auto".to_string(),
                    track: "ch-1".to_string(),
                    time_unit: None,
                    content: LaneContent::Automation(AutomationPoints {
                        id: "auto-1".to_string(),
                        target: AutomationTarget {
                            parameter: Some("ch-1/Volume".to_string()),
                            expression: None,
                            channel: None,
                            key: None,
                            controller: None,
                        },
                        unit: Some(dawfile_dawproject::AutomationUnit::Decibel),
                        points: vec![
                            AutomationPoint {
                                time: 0.0,
                                value: 0.8,
                                interpolation: Interpolation::Linear,
                            },
                            AutomationPoint {
                                time: 4.0,
                                value: 1.0,
                                interpolation: Interpolation::Hold,
                            },
                        ],
                    }),
                },
                Lane {
                    id: "lane-expr".to_string(),
                    track: "track-1".to_string(),
                    time_unit: None,
                    content: LaneContent::Automation(AutomationPoints {
                        id: "auto-expr".to_string(),
                        target: AutomationTarget {
                            parameter: None,
                            expression: Some(ExpressionType::PitchBend),
                            channel: Some(0),
                            key: None,
                            controller: None,
                        },
                        unit: Some(dawfile_dawproject::AutomationUnit::Normalized),
                        points: vec![AutomationPoint {
                            time: 0.0,
                            value: 0.5,
                            interpolation: Interpolation::Hold,
                        }],
                    }),
                },
                Lane {
                    id: "lane-markers".to_string(),
                    track: "track-master".to_string(),
                    time_unit: None,
                    content: LaneContent::Markers(vec![Marker {
                        time: 0.0,
                        name: "Intro".to_string(),
                        color: Some("#00FF00".to_string()),
                        comment: Some("start here".to_string()),
                    }]),
                },
            ],
            tempo_automation: vec![
                TempoPoint {
                    time: 0.0,
                    bpm: 132.5,
                    interpolation: Interpolation::Hold,
                },
                TempoPoint {
                    time: 8.0,
                    bpm: 140.0,
                    interpolation: Interpolation::Linear,
                },
            ],
            time_sig_automation: vec![TimeSignaturePoint {
                time: 4.0,
                numerator: 3,
                denominator: 8,
            }],
        }),
        scenes: vec![Scene {
            id: "scene-1".to_string(),
            name: Some("Verse".to_string()),
            color: None,
            comment: Some("first verse".to_string()),
            content: None,
        }],
    }
}

#[test]
fn round_trip_minimal() {
    let original = minimal_project();

    let bytes =
        dawfile_dawproject::serialize_project_bytes(&original).expect("serialize should succeed");
    assert!(!bytes.is_empty());

    let restored = dawfile_dawproject::parse_project_bytes(&bytes)
        .expect("parse round-tripped bytes should succeed");

    // Transport
    assert_eq!(restored.version, "1.0");
    assert!((restored.transport.tempo - 132.5).abs() < 0.001);
    assert_eq!(restored.transport.numerator, 3);
    assert_eq!(restored.transport.denominator, 8);

    // Application
    assert_eq!(restored.application.unwrap().name, "FastTrackStudio");

    // Metadata
    let meta = restored.metadata.unwrap();
    assert_eq!(meta.title.as_deref(), Some("Test Session"));

    // Tracks
    assert_eq!(restored.tracks.len(), 2);
    let bass = &restored.tracks[0];
    assert_eq!(bass.name, "Bass");
    assert_eq!(bass.comment.as_deref(), Some("low end"));
    assert_eq!(bass.content_types, vec![ContentType::Notes]);
    let ch = bass.channel.as_ref().unwrap();
    assert!((ch.volume - 0.8).abs() < 0.001);
    assert!((ch.pan - (-0.25)).abs() < 0.001);
    assert!(ch.solo);
    assert_eq!(ch.destination.as_deref(), Some("ch-master"));
    assert_eq!(ch.sends.len(), 1);
    assert_eq!(ch.sends[0].destination, "ch-master");
    assert!(ch.sends[0].enabled);

    // Arrangement
    let arr = restored.arrangement.unwrap();
    assert_eq!(arr.time_unit, TimeUnit::Beats);
    assert_eq!(arr.lanes.len(), 4);

    // Clips lane
    if let LaneContent::Clips(clips) = &arr.lanes[0].content {
        let clip = &clips[0];
        assert_eq!(clip.name.as_deref(), Some("Intro"));
        assert_eq!(clip.comment.as_deref(), Some("first clip"));
        assert!(clip.enabled);
        assert!((clip.play_start.unwrap() - 0.5).abs() < 0.001);
        assert!((clip.play_stop.unwrap() - 3.5).abs() < 0.001);
        assert!((clip.fade_in_time.unwrap() - 0.25).abs() < 0.001);
        assert!((clip.fade_out_time.unwrap() - 0.5).abs() < 0.001);
        assert_eq!(clip.fade_time_unit, Some(TimeUnit::Beats));
        if let ClipContent::Notes(notes) = &clip.content {
            assert_eq!(notes.len(), 2);
            assert_eq!(notes[1].key, 43);
            assert!(notes[1].release_velocity.is_some());
        } else {
            panic!("expected Notes");
        }
    } else {
        panic!("expected Clips lane");
    }

    // Automation lane with unit
    if let LaneContent::Automation(pts) = &arr.lanes[1].content {
        assert_eq!(pts.target.parameter.as_deref(), Some("ch-1/Volume"));
        assert_eq!(pts.unit, Some(dawfile_dawproject::AutomationUnit::Decibel));
        assert_eq!(pts.points.len(), 2);
        assert_eq!(pts.points[0].interpolation, Interpolation::Linear);
    } else {
        panic!("expected Automation lane");
    }

    // Expression automation lane
    if let LaneContent::Automation(pts) = &arr.lanes[2].content {
        assert_eq!(pts.target.expression, Some(ExpressionType::PitchBend));
        assert_eq!(pts.target.channel, Some(0));
    } else {
        panic!("expected expression Automation lane");
    }

    // Markers lane
    if let LaneContent::Markers(markers) = &arr.lanes[3].content {
        assert_eq!(markers[0].name, "Intro");
        assert_eq!(markers[0].comment.as_deref(), Some("start here"));
    } else {
        panic!("expected Markers lane");
    }

    // Tempo automation
    assert_eq!(arr.tempo_automation.len(), 2);
    assert!((arr.tempo_automation[0].bpm - 132.5).abs() < 0.001);
    assert!((arr.tempo_automation[1].bpm - 140.0).abs() < 0.001);
    assert_eq!(arr.tempo_automation[1].interpolation, Interpolation::Linear);

    // Time signature automation
    assert_eq!(arr.time_sig_automation.len(), 1);
    assert_eq!(arr.time_sig_automation[0].numerator, 3);
    assert_eq!(arr.time_sig_automation[0].denominator, 8);

    // Arrangement metadata
    assert_eq!(arr.name.as_deref(), Some("Main Arrangement"));

    // Scenes
    assert_eq!(restored.scenes.len(), 1);
    assert_eq!(restored.scenes[0].comment.as_deref(), Some("first verse"));
}

#[test]
fn round_trip_builtin_devices_and_warps() {
    use dawfile_dawproject::{parse_project_bytes, serialize_project_bytes};

    let project = DawProject {
        version: "1.0".to_string(),
        application: None,
        metadata: None,
        transport: Transport {
            tempo: 120.0,
            numerator: 4,
            denominator: 4,
        },
        tracks: vec![Track {
            id: "t1".to_string(),
            name: "Audio".to_string(),
            color: None,
            comment: None,
            content_types: vec![ContentType::Audio],
            loaded: true,
            channel: Some(Channel {
                id: "ch-voice".to_string(),
                role: ChannelRole::Voice,
                audio_channels: 2,
                destination: None,
                blend_mode: None,
                volume: 1.0,
                pan: 0.0,
                muted: false,
                solo: false,
                sends: vec![],
                devices: vec![
                    Device {
                        name: "MyEQ".to_string(),
                        format: DeviceFormat::Equalizer,
                        device_role: None,
                        plugin_id: None,
                        vendor: Some("Bitwig".to_string()),
                        plugin_version: Some("1.2.3".to_string()),
                        device_id: Some("eq-1".to_string()),
                        plugin_path: None,
                        enabled: true,
                        loaded: true,
                        parameters: vec![],
                        builtin_content: BuiltinDeviceContent::Equalizer(vec![
                            EqBand {
                                id: "band-1".to_string(),
                                band_type: EqBandType::Bell,
                                order: Some(2),
                                freq: Some(1000.0),
                                gain: Some(3.0),
                                q: Some(0.7),
                                enabled: true,
                            },
                            EqBand {
                                id: "band-2".to_string(),
                                band_type: EqBandType::HighPass,
                                order: None,
                                freq: Some(80.0),
                                gain: None,
                                q: None,
                                enabled: false,
                            },
                        ]),
                        state: None,
                    },
                    Device {
                        name: "MyComp".to_string(),
                        format: DeviceFormat::Compressor,
                        device_role: None,
                        plugin_id: None,
                        vendor: None,
                        plugin_version: None,
                        device_id: None,
                        plugin_path: None,
                        enabled: true,
                        loaded: true,
                        parameters: vec![],
                        builtin_content: BuiltinDeviceContent::Compressor(CompressorParams {
                            threshold: Some(-12.0),
                            ratio: Some(4.0),
                            attack: Some(10.0),
                            release: Some(100.0),
                            input_gain: None,
                            output_gain: Some(3.0),
                            auto_makeup: Some(true),
                        }),
                        state: None,
                    },
                    Device {
                        name: "MyGate".to_string(),
                        format: DeviceFormat::NoiseGate,
                        device_role: None,
                        plugin_id: None,
                        vendor: None,
                        plugin_version: None,
                        device_id: None,
                        plugin_path: None,
                        enabled: true,
                        loaded: true,
                        parameters: vec![],
                        builtin_content: BuiltinDeviceContent::NoiseGate(NoiseGateParams {
                            threshold: Some(-40.0),
                            range: Some(-80.0),
                            ratio: Some(10.0),
                            attack: Some(5.0),
                            release: Some(50.0),
                        }),
                        state: None,
                    },
                ],
            }),
            children: vec![],
        }],
        arrangement: Some(Arrangement {
            id: "arr".to_string(),
            name: None,
            color: None,
            comment: None,
            time_unit: TimeUnit::Beats,
            markers: vec![],
            lanes: vec![Lane {
                id: "lane-audio".to_string(),
                track: "t1".to_string(),
                time_unit: None,
                content: LaneContent::Clips(vec![Clip {
                    id: "clip-audio".to_string(),
                    time: 0.0,
                    duration: 8.0,
                    time_unit: None,
                    content_time_unit: None,
                    name: None,
                    color: None,
                    comment: None,
                    fade_in_time: None,
                    fade_out_time: None,
                    fade_time_unit: None,
                    enabled: true,
                    play_start: None,
                    play_stop: None,
                    reference: None,
                    loop_settings: None,
                    content: ClipContent::Audio(AudioContent {
                        file: Some(FileReference {
                            path: "audio/kick.wav".to_string(),
                            external: false,
                        }),
                        sample_rate: Some(44100),
                        channels: Some(1),
                        duration: Some(88200.0),
                        algorithm: Some("beats".to_string()),
                        warps: Warps {
                            content_time_unit: Some(TimeUnit::Seconds),
                            warps: vec![],
                        },
                    }),
                }]),
            }],
            tempo_automation: vec![],
            time_sig_automation: vec![],
        }),
        scenes: vec![],
    };

    let bytes = serialize_project_bytes(&project).expect("serialize");
    let r = parse_project_bytes(&bytes).expect("parse");

    let arr = r.arrangement.unwrap();
    if let LaneContent::Clips(clips) = &arr.lanes[0].content {
        if let ClipContent::Audio(audio) = &clips[0].content {
            assert_eq!(
                audio.file.as_ref().map(|f| f.path.as_str()),
                Some("audio/kick.wav")
            );
            assert_eq!(audio.warps.content_time_unit, Some(TimeUnit::Seconds));
        } else {
            panic!("expected Audio");
        }
    }

    let ch = r.tracks[0].channel.as_ref().unwrap();
    assert_eq!(ch.role, ChannelRole::Voice);

    let eq_dev = &ch.devices[0];
    assert_eq!(eq_dev.vendor.as_deref(), Some("Bitwig"));
    assert_eq!(eq_dev.plugin_version.as_deref(), Some("1.2.3"));
    assert_eq!(eq_dev.device_id.as_deref(), Some("eq-1"));
    if let BuiltinDeviceContent::Equalizer(bands) = &eq_dev.builtin_content {
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].band_type, EqBandType::Bell);
        assert_eq!(bands[0].order, Some(2));
        assert!((bands[0].freq.unwrap() - 1000.0).abs() < 0.01);
        assert!((bands[0].gain.unwrap() - 3.0).abs() < 0.01);
        assert!(bands[0].enabled);
        assert_eq!(bands[1].band_type, EqBandType::HighPass);
        assert!(!bands[1].enabled);
    } else {
        panic!("expected Equalizer content");
    }

    let comp_dev = &ch.devices[1];
    if let BuiltinDeviceContent::Compressor(p) = &comp_dev.builtin_content {
        assert!((p.threshold.unwrap() - (-12.0)).abs() < 0.01);
        assert!((p.ratio.unwrap() - 4.0).abs() < 0.01);
        assert_eq!(p.auto_makeup, Some(true));
    } else {
        panic!("expected Compressor content");
    }

    let gate_dev = &ch.devices[2];
    if let BuiltinDeviceContent::NoiseGate(p) = &gate_dev.builtin_content {
        assert!((p.threshold.unwrap() - (-40.0)).abs() < 0.01);
        assert!((p.range.unwrap() - (-80.0)).abs() < 0.01);
    } else {
        panic!("expected NoiseGate content");
    }
}

#[test]
fn feature_support_is_read_write() {
    use daw_proto::capability::Capability;

    let support = feature_support();
    assert!(support.can_read(Capability::Tracks));
    assert!(support.can_write(Capability::Tracks));
    assert!(support.can_read(Capability::Midi));
    assert!(support.can_write(Capability::Automation));
}
