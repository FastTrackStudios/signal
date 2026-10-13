# Stem-analysis shell — separation models plus the tooling to drive them.
# Enter with `nix develop .#stems`.
#
# Separation itself is Python: the models that matter (BS-RoFormer,
# Mel-Band RoFormer, MDX23C DrumSep) ship as PyTorch checkpoints and are
# reached through `audio-separator`. Rust orchestrates and does all the
# measurement; this shell exists so the Python side is reproducible
# rather than rebuilt by hand each time.
#
# Every export below is here because its absence produced a *silent*
# failure, not an error:
#
#   * Without the host driver on LD_LIBRARY_PATH, PyTorch reports
#     `torch.cuda.is_available() == False`, nothing errors, and
#     separation runs on CPU roughly a hundred times slower.
#   * PyPI wheels are built for an FHS system, so numpy and torch need
#     libz.so.1 and libstdc++.so.6 put on the path explicitly. Symptom is
#     an ImportError from inside a dependency, not from your own code.
#   * Passing tool paths as `/nix/store/...` env vars works until a GC
#     removes them, after which downloads "fail" in a way that looks
#     exactly like the source material being unavailable. Take tools from
#     PATH in this shell instead.
{ ... }:
{
  perSystem = { pkgs, lib, ... }: {
    devShells.stems = pkgs.mkShell {
      packages = with pkgs; [
        # uv runs audio-separator / demucs in their own environments.
        # These are PyPI-only, with heavy pinned dependency trees that do
        # not belong in nixpkgs.
        uv
        python3
        # Separation shells out to ffmpeg for anything its own loader
        # cannot decode; ffprobe verifies what actually landed on disk.
        ffmpeg
        sox
        flac
      ];

      shellHook = ''
        # The host NVIDIA driver. Necessarily impure: it has to match the
        # running kernel module, so it cannot come from nixpkgs.
        #
        # If CUDA is unavailable despite this, run `nvidia-smi` BEFORE
        # suspecting anything here. A NixOS upgrade installs a new driver
        # while the running kernel keeps the old module, which surfaces as
        #   Error 803: system has unsupported display driver / cuda driver
        #   combination
        # and is fixed by rebooting. Diagnosing that as a library-path
        # problem cost a couple of hours once already.
        export LD_LIBRARY_PATH="/run/opengl-driver/lib:${lib.makeLibraryPath [
          pkgs.stdenv.cc.cc.lib   # libstdc++.so.6 — torch
          pkgs.zlib               # libz.so.1 — numpy
        ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

        # audio-separator 0.47 does not declare audioread, and imports it
        # unconditionally — so the CLI dies on import without this.
        # Pinned here so every caller gets the same working set.
        export STEMS_UV_ARGS="--with audio-separator[gpu] --with audioread --with librosa --with soundfile"

        echo "  signal — stem analysis shell"
        echo "  ─────────────────────────────────────────────"
        echo "  uv run \$STEMS_UV_ARGS audio-separator --list_models"
        echo "  multitracks: /run/media/Development/mir-datasets/data/cambridge-mt"
        echo ""
      '';
    };
  };
}
