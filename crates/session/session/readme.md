# Session Cell

A control surface cell that presents DAW information and provides transport controls.

## Overview

The session cell is a **peer cell** that:
- Connects to the DAW cell via the host's SHM mechanism
- Uses `daw-control` as the public API to communicate with any DAW implementation
- Provides a control surface interface for transport commands (play, stop, etc.)
- Presents DAW state and information from keyflow modules

## Architecture

```
┌─────────────────┐
│   Session Cell  │  ← Uses daw-control API
│  (control surf) │
└────────┬────────┘
         │
         │ SHM / roam RPC
         │
┌────────▼────────┐
│      Host       │  ← Routes calls
│                 │
└────────┬────────┘
         │
         │ SHM / spawn
         │
┌────────▼────────┐
│   DAW Cell      │  ← Implements DAW Protocol
│ (standalone/    │     (play, stop, project info)
│  reaper/etc)    │
└─────────────────┘
```

## Dependencies

- **daw-control**: The public API for DAW communication (the only DAW dependency)
- **daw-proto**: Protocol types
- **roam**: RPC framework

**Note**: Session does NOT depend on `daw-standalone` or any specific DAW implementation.

## Usage

The session cell is spawned by the host as a peer process. It receives spawn arguments
to connect to the host's SHM segment, then uses `daw-control` to:

1. Discover the DAW cell
2. Get project information
3. Call transport commands

### Example (intended API)

```rust
use daw_control::Daw;

// Initialize daw-control with connection handle from host
let handle = /* received from host */;
Daw::init(handle)?;

// Get current project
let project = Daw::current_project().await?;

// Control transport
project.transport().play().await?;
project.transport().stop().await?;
```

## Integration Tests

See `tests/integration_tests.rs` for examples of how the session uses daw-control
to communicate with the DAW.

## Future

- Desktop app frontend (connects to session cell)
- Web app frontend (connects via WebSocket to session cell)
