+++
title = "DAW"
description = "REAPER integration and DAW bridge for FastTrackStudio"
+++

DAW is the REAPER integration and service layer for FastTrackStudio.

It provides bidirectional transport control, marker-driven chart navigation,
MIDI routing, named FTS screensets, project-file tooling, and real-time state
broadcast through vox services.

## Overview

- [Spec](/spec/) — Specifications and requirements
- [Getting Started](/getting-started/) — Building and running the REAPER extension

## Operations

- [REAPER Desktop Renderer Polling](/reaper-desktop-renderer-polling/) — Linux embedded desktop panel timer cadence
- [Dioxus Renderer Event Converters](/dioxus-renderer-event-converters/) — Mixed native and desktop renderer event dispatch contract

## Features

- **Bidirectional transport sync** — Play, stop, seek, and loop between REAPER and FTS tools
- **Marker and region mapping** — Chart sections linked to REAPER markers
- **MIDI routing** — Controller integration through REAPER's MIDI infrastructure
- **Session state broadcast** — Real-time state over vox service streams
- **FTS screensets** — Named workspace, track visibility, and selection snapshots
- **RPP file parsing** — Read and manipulate REAPER project files programmatically
