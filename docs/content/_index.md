+++
title = "Signal"
description = "Audio plugins and DSP framework for FastTrackStudio"
+++

Signal is the audio signal processing and plugin framework for FastTrackStudio.

It targets CLAP and VST3 from a single Rust codebase via nih-plug, with GPU-accelerated custom UIs built on the FTS design system.

## Overview

- [Getting Started](/getting-started/) — Building and running Signal plugins
- [Architecture](/architecture/) — How Signal is structured
- [Sampler Roadmap](/sampler-roadmap/) — Kontakt-class sampler playback and product plan

## Goals

- **Single codebase, multiple formats** — CLAP and VST3 output from one Rust source via nih-plug
- **Cross-platform** — Linux, macOS, and Windows as equal targets
- **FTS protocol integration** — Session-aware parameters that sync with the broader ecosystem
- **GPU-accelerated UIs** — Custom plugin interfaces built with the FTS design system
