# Draft: redirect-URI question for support@tone3000.com

Blocks the auth design for the plugin/desktop path (build order steps 4–5).
Send before writing the redirect handling; the answer picks the mechanism.

---

**Subject:** Redirect URI options for a desktop/plugin integration (open-source)

Hi,

We're adding TONE3000 support to FastTrackStudio, a GPL-3.0 open-source
audio application (https://github.com/FastTrackStudios). Users would browse
and load NAM captures from their TONE3000 account into our NAM library.

We plan to use the `prompt=select_tone` flow so browsing happens in your own
picker rather than against the search endpoint.

Our product runs in three shapes, and we want to register the right redirect
URIs up front rather than guess:

1. **Headless engine + browser UI on a LAN.** A background process serves our
   UI over plain HTTP to other devices (phone/tablet) on the local network.
   We'd catch the redirect on an HTTP route the engine already serves.

2. **Audio plugin (VST3/CLAP) inside a third-party DAW.** Our plugin UI
   renders through a GPU vector renderer, not a browser engine, so it cannot
   host the authorization page. We'd open the system browser and catch the
   result out of process.

3. **iOS app**, where we'd use `ASWebAuthenticationSession` with a deep link.

The docs say "If you've registered redirect URIs in settings, only those will
be accepted", and recommend a deep link such as `myapp://callback` for native
apps. Three questions:

- **Loopback with a variable port.** Are loopback redirect URIs such as
  `http://127.0.0.1:<port>/callback` accepted, with the port varying per run
  as RFC 8252 §7.3 describes? This is the simplest option for cases 1 and 2,
  but only if the port need not be fixed at registration.

- **Custom scheme.** If loopback is not supported, can we register a single
  custom scheme (e.g. `fasttrackstudio://t3k/callback`) and use it from all
  three shapes, including the desktop plugin?

- **Non-loopback LAN addresses.** For case 1 the browser may be on a
  different device from the engine, so the redirect would target a private
  address like `http://192.168.1.20:4040/...` over plain HTTP. Is that
  permitted, or should we use the LAN-relay approach from the Devo demo in
  github.com/tone-3000/api instead?

Also, to confirm we're on the right footing: we're non-commercial and
open-source, so we understand we're on the free tier — OAuth plus the bounded
list endpoints, and no reliance on `/tones/search`. We'll carry
"Powered by TONE3000" with a link, and we persist each tone's creator,
licence and tone URL alongside the downloaded file so attribution survives.

Thanks,
Cody Wright — FastTrackStudio
