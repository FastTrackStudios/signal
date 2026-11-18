#!/usr/bin/env python3
"""Fold each zone's group tune into its zone tune.

    python3 fold_group_tune.py <_ksp/<instrument> dir> > zones_tuned.tsv

Kontakt plays a zone at group tune × zone tune. CSS tunes every playable
group alike (1.0052122, +9 cents: the strings spec's `master_tune_cents`),
but CSW tunes groups one by one (Solo Oboe: only its marcato-legato groups;
Solo Clarinet: all but the trills, by 1.005793) and CSB not at all. For
those the library-wide master tune is 0 and the group tune belongs in each
zone: this writes zones.tsv with `tune` = zone tune × its group's tune, for
`signalpack zones sync-tsv --from-tsv`.
"""
import csv, json, os, sys


def main():
    d = sys.argv[1]
    groups = json.load(open(os.path.join(d, "groups.json")))
    tune = [float(g.get("tune", 1) or 1) for g in groups]
    rows = csv.DictReader(open(os.path.join(d, "zones.tsv")), delimiter="\t")
    assert rows.fieldnames is not None
    out = csv.DictWriter(sys.stdout, fieldnames=rows.fieldnames, delimiter="\t", lineterminator="\n")
    out.writeheader()
    for r in rows:
        g = int(r["group"])
        r["tune"] = repr(float(r["tune"]) * (tune[g] if 0 <= g < len(tune) else 1.0))
        out.writerow(r)


if __name__ == "__main__":
    main()
