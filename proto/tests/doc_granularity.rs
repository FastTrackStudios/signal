//! MEASURE (live S4, the Captain's Q3): which doc granularity for Signal's
//! live docs — one doc per entity, one doc for the whole library, or one doc
//! per setlist?
//!
//! On the demo library (`signal_proto::doc::seed`: Task's dev-seed songs and
//! setlists on Signal's megarigs, 2 rigs · 28 patches · 13 songs · 2
//! setlists) this measures, for each layout:
//!
//! 1. size — docs, copies of each entity, snapshot bytes;
//! 2. the update bytes of typical edits, and how many docs each touches;
//! 3. sync and merge cost — a cold join of the whole library, a cold join of
//!    just what one gig needs, and merging one edit;
//! 4. concurrent edits — an agent edits a setlist while the Captain tweaks a
//!    patch: does everything land, do replicas converge, does any entity end
//!    up with copies that disagree?
//!
//! and prints the tables `docs/adr/0001-signal-live-doc-granularity.md`
//! quotes. Run with:
//!
//! `cargo test -p signal-proto --features doc --test doc_granularity -- --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_precision_loss
)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use signal_proto::default_profile::LibrarySettings;
use signal_proto::doc::library::{Layout, Library, PatchRef, build};
use signal_proto::doc::{DocEntity, SignalDoc, seed};
use signal_proto::overrides::NodeOverrideOp;
use signal_proto::profile::{Patch, PatchId, ProfileId};
use signal_proto::seed_id;
use signal_proto::song::SceneSource;
use signal_proto::{ParameterValue, setlist::Setlist};

const SUNDAY: &str = "sunday";
const RELEASE: &str = "release";

fn setlist_key(slug: &str) -> String {
    format!("setlist:{}", seed::setlist_id(slug))
}

fn song_key(title: &str) -> String {
    format!("song:{}", seed::song_id(title))
}

/// A library's docs in one layout, kept in step with edits to it.
struct Docs {
    layout: Layout,
    docs: BTreeMap<String, SignalDoc>,
}

/// What one edit cost.
#[derive(Default)]
struct EditCost {
    docs: usize,
    bytes: usize,
}

impl Docs {
    fn new(layout: Layout, library: &Library) -> Self {
        Self {
            layout,
            docs: build(layout, library, "seed").unwrap(),
        }
    }

    fn fork(&self) -> Self {
        Self {
            layout: self.layout,
            docs: self
                .docs
                .iter()
                .map(|(n, d)| (n.clone(), d.fork()))
                .collect(),
        }
    }

    /// Bring every doc in step with `library`, committing under `origin`:
    /// the update bytes each changed doc emits.
    fn sync(&mut self, library: &Library, origin: &str) -> EditCost {
        let mut cost = EditCost::default();
        for (name, share) in self.layout.contents(library) {
            let doc = self.docs.entry(name).or_default();
            let before = doc.version();
            doc.write_library(&share).unwrap();
            doc.commit(origin);
            if doc.version() != before {
                cost.docs += 1;
                cost.bytes += doc.updates_since(&before).unwrap().len();
            }
        }
        cost
    }

    fn snapshot_sizes(&self) -> Vec<usize> {
        self.docs
            .values()
            .map(|d| d.snapshot().unwrap().len())
            .collect()
    }

    /// The docs a device needs to have `keys` — the docs holding them.
    fn docs_holding(&self, keys: &BTreeSet<String>) -> Vec<&str> {
        self.docs
            .iter()
            .filter(|(_, doc)| {
                signal_proto::doc::EntityKind::ALL
                    .iter()
                    .any(|k| doc.keys(*k).iter().any(|key| keys.contains(key)))
            })
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// Exchange everything with `other`, both ways, the way two peers would.
    fn merge_with(&mut self, other: &mut Self) {
        let names: BTreeSet<String> = self.docs.keys().chain(other.docs.keys()).cloned().collect();
        for name in names {
            match (self.docs.get(&name), other.docs.get(&name)) {
                (Some(a), Some(b)) => {
                    let (av, bv) = (a.version(), b.version());
                    a.import(&b.updates_since(&av).unwrap()).unwrap();
                    b.import(&a.updates_since(&bv).unwrap()).unwrap();
                }
                (Some(a), None) => {
                    let copy = SignalDoc::from_bytes(&a.snapshot().unwrap()).unwrap();
                    other.docs.insert(name, copy);
                }
                (None, Some(b)) => {
                    let copy = SignalDoc::from_bytes(&b.snapshot().unwrap()).unwrap();
                    self.docs.insert(name, copy);
                }
                (None, None) => {}
            }
        }
    }

    /// Every doc as JSON — two replicas have converged when these agree.
    fn dumps(&self) -> BTreeMap<String, serde_json::Value> {
        self.docs
            .iter()
            .map(|(n, d)| (n.clone(), d.dump()))
            .collect()
    }

    /// Each entity key with every copy of it across the docs, as JSON.
    fn copies(&self) -> BTreeMap<String, Vec<serde_json::Value>> {
        let mut out: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        for doc in self.docs.values() {
            let dump = doc.dump();
            for kind in signal_proto::doc::EntityKind::ALL {
                if let Some(serde_json::Value::Object(entities)) = dump.get(kind.as_str()) {
                    for (key, value) in entities {
                        out.entry(key.clone()).or_default().push(value.clone());
                    }
                }
            }
        }
        out
    }

    /// Read one entity, from whichever doc holds it (the first, by name).
    fn read<E: DocEntity>(&self, key: &str) -> Option<E> {
        self.docs.values().find_map(|d| d.get::<E>(key).unwrap())
    }
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn time(runs: usize, mut f: impl FnMut()) -> Duration {
    median(
        (0..runs)
            .map(|_| {
                let start = Instant::now();
                f();
                start.elapsed()
            })
            .collect(),
    )
}

fn us(d: Duration) -> String {
    format!("{:.0} µs", d.as_secs_f64() * 1e6)
}

fn kib(bytes: usize) -> String {
    if bytes < 4096 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

// ── the edits ───────────────────────────────────────────────────────────

fn set_first_override(patch: &mut Patch, value: f32) {
    if let Some(ov) = patch.overrides.first_mut()
        && let NodeOverrideOp::Set { value: v } = &mut ov.op
    {
        *v = ParameterValue::new(value);
    }
}

type Edit = (&'static str, fn(&mut Library));

const EDITS: &[Edit] = &[
    ("Captain tweaks a patch (amp gain)", |lib| {
        set_first_override(lib.patch_mut("patch:worship-lead").unwrap(), 0.78);
    }),
    ("rename a patch", |lib| {
        lib.patch_mut("patch:worship-ambient").unwrap().name = "Ambient Swell".to_owned();
    }),
    ("agent reorders a setlist", |lib| {
        let set = lib.setlist_mut(&setlist_key(SUNDAY)).unwrap();
        let last = set.entries.pop().unwrap();
        set.entries.insert(1, last);
    }),
    ("agent adds a song to a setlist", |lib| {
        let set = lib.setlist_mut(&setlist_key(RELEASE)).unwrap();
        set.add_entry(seed::entry(RELEASE, "Open Doors"));
    }),
    ("change a song scene's patch", |lib| {
        let song = lib.song_mut(&song_key("Anchor Line")).unwrap();
        song.scenes[2].source = SceneSource::Patch {
            patch_id: PatchId::from(seed_id("worship-drive")),
        };
    }),
    ("new patch", |lib| {
        let mut patch = lib.patch("patch:allround-ambient").unwrap().clone();
        patch = patch.duplicate(PatchId::from(seed_id("allround-shimmer")), "Shimmer");
        lib.patches.push(patch.with_slug("allround-shimmer"));
        let all_around = format!("profile:{}", seed::all_around_id());
        lib.profile_mut(&all_around)
            .unwrap()
            .patches
            .push(PatchRef {
                id: "patch:allround-shimmer".to_owned(),
            });
    }),
    (
        "Captain edits All-Around's default patch (inheriting songs follow)",
        |lib| {
            set_first_override(lib.patch_mut("patch:allround-clean").unwrap(), 0.3);
        },
    ),
    ("switch the library's default profile to Worship", |lib| {
        lib.settings = Some(LibrarySettings::with_default_profile(ProfileId::from(
            seed_id("worship-profile"),
        )));
    }),
];

// ── the measurement ─────────────────────────────────────────────────────

#[test]
fn measure_doc_granularity() {
    let library = seed::demo_library();
    let entity_json: usize = library
        .rigs
        .iter()
        .map(|e| serde_json::to_vec(e).unwrap().len())
        .chain(
            library
                .patches
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len()),
        )
        .chain(
            library
                .songs
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len()),
        )
        .chain(
            library
                .profiles
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len()),
        )
        .chain(
            library
                .setlists
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len()),
        )
        .sum();
    let inheriting = library
        .songs
        .iter()
        .flat_map(|s| &s.scenes)
        .filter(|s| s.inherits_default())
        .count();
    println!(
        "\n## Library\n\n{} rigs · {} patches · {} profiles · {} songs ({} scenes inherit All-Around) · {} setlists · library:settings = {} entities, {} as JSON\n",
        library.rigs.len(),
        library.patches.len(),
        library.profiles.len(),
        library.songs.len(),
        inheriting,
        library.setlists.len(),
        library.len(),
        kib(entity_json),
    );
    for setlist in &library.setlists {
        println!(
            "{} references {}\n",
            setlist.name,
            library.patch_refs(setlist).join(", ")
        );
    }

    // 1. size
    println!("## 1. Size\n");
    println!(
        "| layout | docs | entity copies (unique) | snapshot total | largest doc | median doc |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    for layout in Layout::ALL {
        let docs = Docs::new(layout, &library);
        let copies: usize = layout.contents(&library).values().map(Library::len).sum();
        let mut sizes = docs.snapshot_sizes();
        sizes.sort_unstable();
        println!(
            "| {} | {} | {} ({}) | {} | {} | {} |",
            layout.as_str(),
            docs.docs.len(),
            copies,
            library.len(),
            kib(sizes.iter().sum()),
            kib(*sizes.last().unwrap()),
            kib(sizes[sizes.len() / 2]),
        );
    }

    // 2. typical edits
    println!("\n## 2. Typical edits: update bytes (docs touched)\n");
    println!("| edit | per-entity | whole-library | per-setlist |");
    println!("|---|---:|---:|---:|");
    let mut rows: Vec<(String, Vec<String>)> = EDITS
        .iter()
        .map(|(n, _)| ((*n).to_owned(), vec![]))
        .collect();
    for layout in Layout::ALL {
        let mut lib = library.clone();
        let mut docs = Docs::new(layout, &lib);
        for (row, (_, edit)) in rows.iter_mut().zip(EDITS) {
            edit(&mut lib);
            let cost = docs.sync(&lib, "edit");
            row.1.push(format!("{} ({})", kib(cost.bytes), cost.docs));
        }
        // The docs still hold every entity of the edited library.
        let copies = docs.copies();
        for key in lib.keys() {
            assert!(
                copies.contains_key(&key),
                "{}: {key} missing",
                layout.as_str()
            );
        }
    }
    for (name, cells) in &rows {
        println!("| {name} | {} |", cells.join(" | "));
    }

    // 3. sync / merge cost
    println!("\n## 3. Sync and merge cost\n");
    println!(
        "| layout | cold join, whole library: bytes · docs · import+read | cold join, one gig (Sunday Setlist): bytes · docs · import+read | merge one patch tweak |"
    );
    println!("|---|---|---|---|");
    let sunday = library.setlist(&setlist_key(SUNDAY)).unwrap().clone();
    let gig_keys = library.closure(&sunday).keys();
    for layout in Layout::ALL {
        let docs = Docs::new(layout, &library);
        let snapshots: BTreeMap<&str, Vec<u8>> = docs
            .docs
            .iter()
            .map(|(n, d)| (n.as_str(), d.snapshot().unwrap()))
            .collect();
        let join = |names: &[&str]| {
            let bytes: usize = names.iter().map(|n| snapshots[n].len()).sum();
            let t = time(15, || {
                for name in names {
                    let doc = SignalDoc::from_bytes(&snapshots[name]).unwrap();
                    let read = doc.read_library().unwrap();
                    assert!(!read.is_empty());
                }
            });
            format!("{} · {} · {}", kib(bytes), names.len(), us(t))
        };
        let all: Vec<&str> = snapshots.keys().copied().collect();
        let gig = docs.docs_holding(&gig_keys);
        // A gig device needs every entity of the set; check the docs hold them.
        let held: BTreeSet<String> = gig
            .iter()
            .flat_map(|n| {
                signal_proto::doc::EntityKind::ALL
                    .iter()
                    .flat_map(|k| docs.docs[*n].keys(*k))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            gig_keys.is_subset(&held),
            "{}: gig docs miss entities",
            layout.as_str()
        );

        // Merging one edit: the Captain's tweak, into a replica.
        let mut edited = library.clone();
        set_first_override(edited.patch_mut("patch:worship-lead").unwrap(), 0.78);
        let mut captain = docs.fork();
        let before: BTreeMap<String, _> = captain
            .docs
            .iter()
            .map(|(n, d)| (n.clone(), d.version()))
            .collect();
        captain.sync(&edited, "captain");
        let updates: Vec<(String, Vec<u8>)> = captain
            .docs
            .iter()
            .filter(|(n, d)| before.get(*n) != Some(&d.version()))
            .map(|(n, d)| (n.clone(), d.updates_since(&before[n]).unwrap()))
            .collect();
        let merge = median(
            (0..25)
                .map(|_| {
                    let replicas: Vec<(SignalDoc, &Vec<u8>)> = updates
                        .iter()
                        .map(|(name, bytes)| (docs.docs[name].fork(), bytes))
                        .collect();
                    let start = Instant::now();
                    for (replica, bytes) in &replicas {
                        replica.import(bytes).unwrap();
                    }
                    start.elapsed()
                })
                .collect(),
        );
        println!(
            "| {} | {} | {} | {} in {} doc(s) ({}) |",
            layout.as_str(),
            join(&all),
            join(&gig),
            kib(updates.iter().map(|(_, b)| b.len()).sum()),
            updates.len(),
            us(merge),
        );
    }

    // 4. concurrent edits
    println!("\n## 4. Concurrent: an agent edits a setlist while the Captain tweaks a patch\n");
    println!(
        "The agent adds \"Open Doors\" to the Album Release Show and opens the show with \
         \"Brighter Still\"; \
         at the same time the Captain turns up `patch:worship-tremolo` (which Open Doors plays) \
         and renames `patch:worship-lead`. Then the two exchange updates.\n"
    );
    println!(
        "| layout | agent's edit lands | Captain's tweak lands | replicas converge | entities whose copies disagree |"
    );
    println!("|---|---|---|---|---|");
    for layout in Layout::ALL {
        let base = Docs::new(layout, &library);
        let mut agent = base.fork();
        let mut captain = base.fork();

        let mut by_agent = library.clone();
        {
            let set = by_agent.setlist_mut(&setlist_key(RELEASE)).unwrap();
            set.add_entry(seed::entry(RELEASE, "Open Doors"));
            let last = set.entries.remove(set.entries.len() - 2);
            set.entries.insert(0, last);
        }
        agent.sync(&by_agent, "agent");

        let mut by_captain = library.clone();
        {
            let tremolo = by_captain.patch_mut("patch:worship-tremolo").unwrap();
            set_first_override(tremolo, 0.8);
            by_captain.patch_mut("patch:worship-lead").unwrap().name = "Lead (big)".to_owned();
        }
        captain.sync(&by_captain, "captain");

        agent.merge_with(&mut captain);

        let converged = agent.dumps() == captain.dumps();
        let set: Setlist = agent.read(&setlist_key(RELEASE)).unwrap();
        let agent_lands = set.entries.len() == 7
            && set.entries[0].name == "Brighter Still"
            && set.entries[6].name == "Open Doors";
        let tremolo_tweaked = |p: &Patch| {
            matches!(p.overrides.first().map(|o| &o.op),
                Some(NodeOverrideOp::Set { value }) if (value.get() - 0.8).abs() < 1e-6)
        };
        let copies = agent.copies();
        let tremolo_copies: Vec<Patch> = copies["patch:worship-tremolo"]
            .iter()
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .collect();
        let captain_lands = tremolo_copies.iter().all(tremolo_tweaked)
            && agent.read::<Patch>("patch:worship-lead").unwrap().name == "Lead (big)";
        let disagree: Vec<&str> = copies
            .iter()
            .filter(|(_, c)| c.windows(2).any(|w| w[0] != w[1]))
            .map(|(k, _)| k.as_str())
            .collect();
        println!(
            "| {} | {} | {} | {} | {} |",
            layout.as_str(),
            yes(agent_lands),
            if captain_lands {
                "yes".to_owned()
            } else {
                format!(
                    "**no** — {} of {} copies of worship-tremolo kept the old value",
                    tremolo_copies
                        .iter()
                        .filter(|p| !tremolo_tweaked(p))
                        .count(),
                    tremolo_copies.len()
                )
            },
            yes(converged),
            if disagree.is_empty() {
                "none".to_owned()
            } else {
                disagree.join(", ")
            },
        );

        assert!(converged, "{}: replicas must converge", layout.as_str());
        assert!(
            agent_lands,
            "{}: the agent's setlist edit must land",
            layout.as_str()
        );
        match layout {
            Layout::PerEntity | Layout::WholeLibrary => {
                assert!(
                    captain_lands,
                    "{}: the Captain's tweak must land",
                    layout.as_str()
                );
                assert!(
                    disagree.is_empty(),
                    "{}: one copy per entity",
                    layout.as_str()
                );
            }
            // The finding that rules per-setlist out: the agent's write
            // copied the patch into the release show's doc from its own,
            // older view, and nothing reconciles copies across docs.
            Layout::PerSetlist => assert!(
                !captain_lands && disagree.contains(&"patch:worship-tremolo"),
                "per-setlist: expected a stale copy of worship-tremolo"
            ),
        }
    }

    // Within one entity, any layout: field-level merge vs whole-list values.
    println!("\n## 4b. Two people edit the same patch at once (any layout)\n");
    println!("| agent | Captain | result |");
    println!("|---|---|---|");
    let case = |agent_edit: fn(&mut Patch), captain_edit: fn(&mut Patch)| {
        let patch = library.patch("patch:worship-solo").unwrap().clone();
        let a = SignalDoc::new();
        a.put(&patch).unwrap();
        a.commit("seed");
        let c = a.fork();
        let mut pa = patch.clone();
        agent_edit(&mut pa);
        a.put(&pa).unwrap();
        a.commit("agent");
        let mut pc = patch.clone();
        captain_edit(&mut pc);
        c.put(&pc).unwrap();
        c.commit("captain");
        let (av, cv) = (a.version(), c.version());
        a.import(&c.updates_since(&av).unwrap()).unwrap();
        c.import(&a.updates_since(&cv).unwrap()).unwrap();
        assert_eq!(a.dump(), c.dump());
        let merged: Patch = a.get(&patch.doc_key()).unwrap().unwrap();
        (merged, pa, pc)
    };
    let (merged, pa, pc) = case(
        |p| p.name = "Solo (agent)".to_owned(),
        |p| set_first_override(p, 0.95),
    );
    let both = merged.name == pa.name && merged.overrides == pc.overrides;
    println!(
        "| renames it | turns the amp up | {} |",
        if both { "both land" } else { "one lost" }
    );
    assert!(both, "different fields of one patch must both land");
    let (merged, pa, pc) = case(
        |p| set_first_override(p, 0.95),
        |p| {
            if let Some(NodeOverrideOp::Set { value }) = p.overrides.get_mut(1).map(|o| &mut o.op) {
                *value = ParameterValue::new(0.1);
            }
        },
    );
    let both = merged.overrides[0] == pa.overrides[0] && merged.overrides[1] == pc.overrides[1];
    println!(
        "| turns the amp up | turns the delay down | {} |",
        if both {
            "both land"
        } else {
            "one lost: a patch's overrides are one value (last writer wins)"
        }
    );
}

fn yes(b: bool) -> String {
    if b {
        "yes".to_owned()
    } else {
        "**no**".to_owned()
    }
}
