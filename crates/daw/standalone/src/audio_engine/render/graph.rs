//! Routing-graph analysis run once per snapshot (not per block):
//! topological processing order + solo routing mask.

use std::collections::VecDeque;

use super::snapshot::TrackSnapshot;

/// Topological processing order over the ROUTING GRAPH: a track must
/// be fully processed before anything it feeds — its folder parent
/// (children sum upward) and every send destination (busses receive
/// post-fader signal). Depth ordering alone breaks bus mixes: a send
/// landing on an already-processed bus would never be forwarded
/// (the session's whole drum mix flows Drums → DRUM BUS → MIX BUS).
/// Kahn's algorithm; any cycle remainder (feedback loops) appends in
/// index order.
pub(crate) fn topo_order(tracks: &[TrackSnapshot]) -> Vec<usize> {
    let n = tracks.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    let mut edge = |a: usize, b: usize| {
        if a != b {
            adj[a].push(b);
            indeg[b] += 1;
        }
    };
    for (i, t) in tracks.iter().enumerate() {
        if let Some(pi) = t.parent_idx {
            edge(i, pi);
        }
        for snd in &t.sends {
            if let Some(di) = snd.dest_idx {
                edge(i, di);
            }
        }
    }
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &j in &adj[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                queue.push_back(j);
            }
        }
    }
    if order.len() < n {
        for i in 0..n {
            if indeg[i] > 0 {
                order.push(i);
            }
        }
    }
    order
}

/// What a track does while something is soloed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pass {
    /// Silent: not part of what is soloed.
    Off,
    /// Plays, as it always does: what is soloed, what is inside it, and
    /// its way to master.
    On,
    /// Plays INTO what it feeds and nowhere else: its sends run, its
    /// folder does not hear it. A soloed Sub hears the kick's Sum, and the
    /// kick is not heard beside it; a key keys its gate, unheard.
    Feed,
    /// Soloed, with no way of its own to be heard — a trigger, whose only
    /// send is a key: put to master while soloed, so a solo is a listen.
    Monitor,
}

impl Pass {
    /// Whether the track is processed at all.
    pub(crate) const fn passes(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Solo routing: what each track does while something is soloed. `None`
/// when nothing is (everything passes). See [`solo_mask`] for the rules.
pub(crate) fn solo_pass(tracks: &[TrackSnapshot]) -> Option<Vec<Pass>> {
    if !tracks.iter().any(|t| t.soloed) {
        return None;
    }
    let parents: Vec<Option<usize>> = tracks.iter().map(|t| t.parent_idx).collect();
    let routes = |sidechain: bool| -> Vec<Vec<usize>> {
        tracks
            .iter()
            .map(|t| {
                t.sends
                    .iter()
                    .filter(|snd| !snd.muted && snd.to_sidechain == sidechain)
                    .filter_map(|snd| snd.dest_idx)
                    .collect()
            })
            .collect()
    };
    let audio = routes(false);
    let soloed: Vec<bool> = tracks.iter().map(|t| t.soloed).collect();
    let mut pass = with_keys(
        solo_mask(&parents, &audio, &soloed),
        &parents,
        &routes(true),
    );
    // A soloed track with no way of its own to master — no parent send,
    // no hardware out, no audio send — is listened to directly.
    for ((p, t), sends) in pass.iter_mut().zip(tracks).zip(&audio) {
        if t.soloed && !t.parent_send && !t.hw_out && sends.is_empty() {
            *p = Pass::Monitor;
        }
    }
    Some(pass)
}

/// The solo mask over the routing graph:
///
/// - a soloed track plays, and so does everything inside it (soloing a
///   folder solos its children);
/// - so does every track on the way from those to master: folder parents,
///   and the destinations of their (unmuted) sends, and onward from
///   there. A session whose stems reach the mix only through sends to
///   buses (parent send off) went silent when this stopped at the folder
///   parents: the bus was not in the mask, so it was skipped and the
///   soloed audio went nowhere. A track on that path is NOT soloed
///   itself: the other tracks that feed the same bus stay silent;
/// - whatever sends INTO a soloed track (repeatedly: a stem into a bus
///   into a soloed mix bus) plays into it and only into it ([`Pass::Feed`]):
///   soloing a bus plays the bus, soloing a return plays the return, not
///   the dry tracks beside it. What is inside such a feeder plays into it
///   as usual — the kick's Sum still hears its mics.
pub(crate) fn solo_mask(
    parents: &[Option<usize>],
    sends: &[Vec<usize>],
    soloed: &[bool],
) -> Vec<Pass> {
    let n = parents.len();
    let mut feeds = vec![false; n];
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if !soloed[i] && !feeds[i] && sends[i].iter().any(|&d| d < n && (soloed[d] || feeds[d]))
            {
                feeds[i] = true;
                changed = true;
            }
        }
    }
    let inside = |j: usize, of: &[bool]| {
        let mut cur = j;
        for _ in 0..64 {
            match parents[cur] {
                Some(p) if p < n => {
                    if of[p] {
                        return true;
                    }
                    cur = p;
                }
                _ => return false,
            }
        }
        false
    };
    // What is soloed, and everything downstream of it: its way to master.
    let mut on: Vec<bool> = (0..n).map(|j| soloed[j] || inside(j, soloed)).collect();
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| on[i]).collect();
    while let Some(i) = queue.pop_front() {
        let onward = parents[i].into_iter().chain(sends[i].iter().copied());
        for d in onward {
            if d < n && !on[d] {
                on[d] = true;
                queue.push_back(d);
            }
        }
    }
    (0..n)
        .map(|j| {
            if on[j] {
                Pass::On
            } else if feeds[j] {
                Pass::Feed
            } else if inside(j, &feeds) {
                Pass::On
            } else {
                Pass::Off
            }
        })
        .collect()
}

/// The tracks that key a passing track's plugins feed it, with everything
/// inside them: a key is never heard — it goes to channels 3/4 — so a
/// solo cannot be about it, and muting it by the solo would shut the gate
/// it keys. Soloing a kick mic must not silence the kick because its
/// trigger was not soloed.
pub(crate) fn with_keys(
    mut pass: Vec<Pass>,
    parents: &[Option<usize>],
    keys: &[Vec<usize>],
) -> Vec<Pass> {
    let n = pass.len();
    let mut keying = vec![false; n];
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if keying[i] || pass[i].passes() {
                continue;
            }
            let keys_a_pass = keys
                .get(i)
                .is_some_and(|k| k.iter().any(|&d| d < n && (pass[d].passes() || keying[d])));
            let in_a_key = parents
                .get(i)
                .copied()
                .flatten()
                .is_some_and(|p| p < n && keying[p]);
            if keys_a_pass || in_a_key {
                keying[i] = true;
                changed = true;
            }
        }
    }
    for i in 0..n {
        if keying[i] {
            // The key itself only feeds; what is inside a folder of keys
            // sums into it.
            let in_a_key = parents[i].is_some_and(|p| p < n && keying[p]);
            pass[i] = if in_a_key { Pass::On } else { Pass::Feed };
        }
    }
    pass
}

#[cfg(test)]
mod key_tests {
    use super::{Pass, with_keys};

    /// 0 Kick (folder), 1 In (in Kick), 2 Trig folder, 3 T1 Trig (in 2)
    /// keying Kick; 4 a stranger. Soloing In passes Kick (its parent):
    /// the trigger folder feeds it and the trigger in it sums into the
    /// folder — not the stranger. With Kick not passing, nothing keys.
    #[test]
    fn a_key_feeds_what_it_keys() {
        use Pass::{Feed, Off, On};
        let parents = [None, Some(0), None, Some(2), None];
        let keys = [vec![], vec![], vec![0], vec![], vec![]];
        let soloed_in = with_keys(vec![On, On, Off, Off, Off], &parents, &keys);
        assert_eq!(soloed_in, [On, On, Feed, On, Off]);
        let nothing = with_keys(vec![Off, Off, Off, Off, On], &parents, &keys);
        assert_eq!(nothing, [Off, Off, Off, Off, On]);
    }
}

#[cfg(test)]
mod tests {
    use super::{Pass, solo_mask};

    /// The live-bus shape: stems inside instrument folders with their
    /// parent send off, reaching the mix only through a send to a bus in
    /// a separate BUSES folder.
    ///
    /// 0 Drums (folder)   1 Kick -> 5   2 Snare -> 5
    /// 3 Bass  (folder)   4 Bass -> 6
    /// 7 BUSES (folder)   5 Drum bus    6 Bass bus
    fn live_bus() -> (Vec<Option<usize>>, Vec<Vec<usize>>) {
        let parents = vec![
            None,
            Some(0),
            Some(0),
            None,
            Some(3),
            Some(7),
            Some(7),
            None,
        ];
        let sends = vec![
            vec![],
            vec![5],
            vec![5],
            vec![],
            vec![6],
            vec![],
            vec![],
            vec![],
        ];
        (parents, sends)
    }

    fn solo(which: &[usize]) -> Vec<Pass> {
        let (parents, sends) = live_bus();
        let mut soloed = vec![false; parents.len()];
        for &i in which {
            soloed[i] = true;
        }
        solo_mask(&parents, &sends, &soloed)
    }

    fn having(mask: &[Pass], pass: Pass) -> Vec<usize> {
        (0..mask.len()).filter(|&i| mask[i] == pass).collect()
    }

    #[test]
    fn a_soloed_stem_reaches_master_through_its_send() {
        // Kick, its folder, the drum bus and the BUSES folder, and NOT the
        // snare that shares the bus.
        assert_eq!(having(&solo(&[1]), Pass::On), vec![0, 1, 5, 7]);
        assert!(having(&solo(&[1]), Pass::Feed).is_empty());
    }

    #[test]
    fn a_soloed_folder_plays_its_stems_through_their_buses() {
        assert_eq!(having(&solo(&[0]), Pass::On), vec![0, 1, 2, 5, 7]);
    }

    #[test]
    fn a_soloed_bus_hears_what_feeds_it_and_only_through_itself() {
        // Drum bus: the kick and snare feed it — into it only — the bus
        // and its folder play; their own folder, and the bass, do not.
        let mask = solo(&[5]);
        assert_eq!(having(&mask, Pass::On), vec![5, 7]);
        assert_eq!(having(&mask, Pass::Feed), vec![1, 2]);
        assert_eq!(mask[0], Pass::Off);
    }

    /// The kick's Sub, soloed: the kick's Sum (0's child 1, mics 2 and 3
    /// in it) feeds the Sub (4, in Kick 0); the Sum feeds it only — not
    /// Kick beside the Sub — and its mics still sum into it.
    #[test]
    fn a_soloed_sub_hears_its_feed_and_nothing_beside() {
        let parents = [None, Some(0), Some(1), Some(1), Some(0)];
        let sends = [vec![], vec![4], vec![], vec![], vec![]];
        let mask = solo_mask(&parents, &sends, &[false, false, false, false, true]);
        assert_eq!(mask, [Pass::On, Pass::Feed, Pass::On, Pass::On, Pass::On]);
    }

    #[test]
    fn a_muted_send_is_not_a_route() {
        let (parents, mut sends) = live_bus();
        sends[1].clear(); // the kick's send, muted (filtered by solo_pass)
        let mut soloed = vec![false; parents.len()];
        soloed[1] = true;
        assert_eq!(
            having(&solo_mask(&parents, &sends, &soloed), Pass::On),
            vec![0, 1]
        );
    }
}
