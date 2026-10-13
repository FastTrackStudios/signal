// AudioWorkletProcessor that drives `WebRenderer` from the wasm
// bundle. Lives on the audio thread; main thread talks via
// `this.port.postMessage`.
//
// Message kinds:
//   init        { wasmBytes, glueUrl, sampleRate, entry? }   → 'ready'
//               entry: 'keys' constructs the keys-rig worklet
//               (signal-keys-worklet's KeysWorklet) instead of the plain
//               WebRenderer — requires a bundle built from that crate.
//   load_rpp    { text, replyTo? }                   → trackCount
//   take_sources { replyTo }                         → [take, path, …]
//   attach_source { takeGuid, pcm, channels, sampleRate }
//   detach_source { takeGuid }
//   play | pause | stop | seek
//
// Keys-rig kinds (entry: 'keys'):
//   attach_pack { key, bytes, replyTo? }             → true/error string
//               bytes: ArrayBuffer of the .signalpack (post as a
//               transferable: postMessage(msg, [bytes]))
//   open_lanes  { program, replyTo? }                → lane count
//               program: the LaneProgram wire JSON (string)
//   reload_lanes { replyTo? }                        → true/error string
//   midi        { bytes: [status, d1, d2] }          raw 3-byte MIDI
//   all_notes_off | panic
//   track_peaks { replyTo }                          → [peak, …] (post-fader
//               per project track; index 0 is the rig folder = master)
//   set_track_volume { index, volume }               linear, 1.0 = unity
//   set_track_mute   { index, muted }
//
// Messages with a `replyTo` field get a reply `{ replyTo, value }`
// so the main side can `await rpc(...)`.

class DawStandaloneProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.renderer = null;
    this.port.onmessage = (e) => this.handleMessage(e.data);
  }

  reply(msg, value) {
    if (msg.replyTo !== undefined) {
      this.port.postMessage({ replyTo: msg.replyTo, value });
    }
  }

  async handleMessage(msg) {
    switch (msg.kind) {
      case 'init': {
        const wasm = await import(msg.glueUrl);
        await wasm.default(msg.wasmBytes);
        this.renderer = msg.entry === 'keys'
          ? new wasm.KeysWorklet(msg.sampleRate)
          : new wasm.WebRenderer(msg.sampleRate);
        this.port.postMessage({ kind: 'ready' });
        break;
      }
      case 'load_rpp': {
        this.renderer?.loadRppText(msg.text);
        this.reply(msg, this.renderer?.trackCount() ?? 0);
        break;
      }
      case 'take_sources': {
        this.reply(msg, this.renderer?.takeSources() ?? []);
        break;
      }
      case 'paths_to_resolve': {
        this.reply(msg, this.renderer?.pathsToResolve() ?? []);
        break;
      }
      case 'attach_source': {
        this.renderer?.attachAudioSource(msg.takeGuid, msg.pcm, msg.channels, msg.sampleRate);
        break;
      }
      case 'detach_source': {
        this.renderer?.detachAudioSource(msg.takeGuid);
        break;
      }
      // ── Keys-rig kinds (entry: 'keys') ─────────────────────────────
      case 'attach_pack': {
        try {
          this.renderer?.attachPack(msg.key, new Uint8Array(msg.bytes));
          this.reply(msg, true);
        } catch (err) {
          this.reply(msg, String(err));
        }
        break;
      }
      case 'open_lanes': {
        try {
          this.reply(msg, this.renderer?.openLanes(msg.program) ?? 0);
        } catch (err) {
          this.reply(msg, String(err));
        }
        break;
      }
      case 'reload_lanes': {
        try {
          this.renderer?.reloadLanes();
          this.reply(msg, true);
        } catch (err) {
          this.reply(msg, String(err));
        }
        break;
      }
      case 'midi': {
        const [status, d1, d2] = msg.bytes;
        this.renderer?.midi(status, d1 ?? 0, d2 ?? 0);
        break;
      }
      case 'set_track_volume': this.renderer?.setTrackVolume(msg.index, msg.volume); break;
      case 'set_track_mute': this.renderer?.setTrackMute(msg.index, msg.muted); break;
      case 'track_peaks': {
        this.reply(msg, Array.from(this.renderer?.trackPeaks?.() ?? []));
        break;
      }
      case 'all_notes_off': this.renderer?.allNotesOff(); break;
      case 'panic': this.renderer?.panic(); break;
      case 'play':  this.renderer?.play();  break;
      case 'pause': this.renderer?.pause(); break;
      case 'stop':  this.renderer?.stop();  break;
      case 'seek':  this.renderer?.seekSeconds(msg.seconds); break;
    }
  }

  process(inputs, outputs, _params) {
    const out = outputs[0];
    if (!out || out.length === 0) return true;
    const l = out[0];
    const r = out[1] ?? out[0];
    if (!this.renderer) {
      l.fill(0);
      if (r !== l) r.fill(0);
      return true;
    }
    this.renderer.render(l, r);
    return true;
  }
}

registerProcessor('daw-standalone', DawStandaloneProcessor);
