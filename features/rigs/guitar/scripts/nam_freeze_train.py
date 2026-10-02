"""Train a frozen Core: the official NAM pipeline, as its trainer GUI runs it.

    python nam_freeze_train.py <input v3_0_0.wav> <output.wav> <outdir> <basename> <epochs> <name>

`input` is the official v3 training signal and `output` the Core's render of
it (unaligned — the pipeline finds the latency from the signal's blips and
runs its data checks). The default model is the trainer's standard one, the
A2 packed WaveNet, exported as a slimmable container. Prints one JSON line:
{"model": <path>, "esr": <validation ESR>}.
"""

import json
import sys
from pathlib import Path

from nam.models.metadata import UserMetadata
from nam.train import core
from nam.train import metadata as train_metadata


def main() -> int:
    input_path, output_path, outdir, basename, epochs, name = sys.argv[1:7]
    outdir = Path(outdir)
    outdir.mkdir(parents=True, exist_ok=True)
    user_metadata = UserMetadata(name=name, modeled_by="Signal (frozen Core)")
    result = core.train(
        input_path,
        output_path,
        str(outdir),
        epochs=int(epochs),
        latency=None,
        save_plot=True,
        silent=True,
        modelname=basename,
        local=True,
        user_metadata=user_metadata,
    )
    if result is None or result.model is None:
        print(json.dumps({"error": "training failed"}))
        return 1
    result.model.net.export(
        outdir,
        basename=basename,
        user_metadata=user_metadata,
        other_metadata={train_metadata.TRAINING_KEY: result.metadata.model_dump()},
    )
    esr = None
    try:
        esr = result.metadata.validation_esr
    except AttributeError:
        pass
    print(json.dumps({"model": str(outdir / f"{basename}.nam"), "esr": esr}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
