#!/usr/bin/env python3
"""Run one separation model over one file, including side-loaded models.

Driven by `signal-separator`; not intended to be run by hand.

Why this exists rather than calling the `audio-separator` CLI
------------------------------------------------------------
`audio-separator` validates a model filename against its own catalog
*before* looking on disk, so a checkpoint sitting right there is refused:

    ValueError: Model file <name> not found in supported model files

The MDX23C DrumSep checkpoint — the only thing that splits a kit into
kick, snare, toms and cymbals — is not in that catalog and never will
be. `download_model_files` is therefore replaced for exactly that model,
returning the local paths instead of consulting the catalog. Every other
model falls through to the original and is fetched normally.

Output contract
---------------
A JSON object on stdout:

    {"stems": {"Kick": "/path/kick.wav", ...}, "model": "..."}

Written as JSON because the caller needs to know *which stem is which*.
The filenames a separator emits are derived from the input name and the
stem label, and matching them by guessing is how stems end up swapped.
"""

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--input", required=True)
    ap.add_argument("--output-dir", required=True)
    ap.add_argument("--model-dir", required=True)
    ap.add_argument("--checkpoint", required=True, help="checkpoint filename")
    ap.add_argument("--config", default=None, help="yaml filename, for MDXC models")
    ap.add_argument("--model-type", default="MDXC")
    ap.add_argument("--sideload", action="store_true",
                    help="bypass the catalog check for this checkpoint")
    args = ap.parse_args()

    from audio_separator.separator import Separator

    model_dir = Path(args.model_dir)
    out_dir = Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    sep = Separator(
        model_file_dir=str(model_dir),
        output_dir=str(out_dir),
        output_format="WAV",
    )

    if args.sideload:
        ckpt = model_dir / args.checkpoint
        if not ckpt.is_file():
            print(json.dumps({"error": f"checkpoint missing: {ckpt}"}))
            return 1

        original = sep.download_model_files

        def resolve(model_filename):
            # Only OUR checkpoint bypasses the catalog; anything else
            # still resolves normally, so this cannot silently shadow a
            # catalogued model that happens to share a name.
            if Path(str(model_filename)).name != args.checkpoint:
                return original(model_filename)
            return (
                args.checkpoint,
                args.model_type,
                Path(args.checkpoint).stem,
                str(ckpt),
                args.config,
            )

        sep.download_model_files = resolve

    sep.load_model(args.checkpoint)
    produced = sep.separate(args.input)

    # `separate()` returns filenames relative to output_dir. Resolve them
    # and label each by the stem it represents, rather than leaving the
    # caller to infer it from a filename.
    stems = {}
    for name in produced:
        p = out_dir / name if not Path(name).is_absolute() else Path(name)
        stems[stem_label(p.name)] = str(p)

    print(json.dumps({"model": args.checkpoint, "stems": stems}))
    return 0


def stem_label(filename: str) -> str:
    """Recover which stem a produced file is.

    audio-separator names outputs `<input>_(<Stem>)_<model>.wav`, so the
    label is the parenthesised part. Falling back to the whole filename
    is deliberate: a wrong-but-visible key is recoverable, whereas
    quietly mapping two stems onto one key loses a stem.
    """
    if "_(" in filename and ")_" in filename:
        return filename.split("_(", 1)[1].split(")_", 1)[0]
    return Path(filename).stem


if __name__ == "__main__":
    sys.exit(main())
