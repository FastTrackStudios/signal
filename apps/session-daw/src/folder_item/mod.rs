//! Folder items: one item on a folder's row, folded from its children's
//! takes.
//!
//! A kit comped across twenty takes is twenty items on the kick, twenty
//! on the snare and twenty on each tom, and the arrangement becomes
//! unreadable. `flow.drums.comping.folder-items` says the folder shows
//! **one item per take** instead, rendered from the kit's own sources —
//! a view of the takes underneath, owning no audio of its own.
//!
//! Three things make that work, and they are the three modules here:
//!
//! - [`fold`] — many children's peaks onto one column grid, `min(min)` /
//!   `max(max)` per column and per group, with a group-by of role (a
//!   kit's pieces) or side (a Channel's L and R).
//! - [`draw`] — the fold as a picture: the outer envelope dim, the
//!   pieces opaque on top of it in the expression editor's own role
//!   colours; or the two sides as the two halves of one stereo
//!   waveform.
//! - [`cache`] — one recorded scene per `(folder revision, mute mask,
//!   take, zoom bucket, group-by)`, replayed under a transform.
//!
//! The shape came from the prototype on `prototype/folder-items` (issue
//! #27), which rejected translucent fixed-order layers, per-column
//! winners and an amplitude bleed gate on the evidence of its sheets.

pub mod cache;
pub mod draw;
pub mod fold;
pub mod load;
pub mod preview;

use anyrender::{PaintScene, Scene};
use vello::peniko::Color;

pub use cache::{PictureCache, PictureKey};
pub use draw::Place;
pub use fold::{Child, ChildTake, Fold, FoldColumn, Grid, GroupBy, Placement, Side, TakePeaks};

/// The most columns one picture is ever folded onto.
///
/// Past this the picture is coarser than the screen — the same trade
/// `.reapeaks` makes at its finest level, and the alternative is a
/// forty-thousand-fill scene recorded for one row of one frame.
pub const MAX_COLUMNS: usize = 16_384;

/// The reference grid a committed fold fixture is written on.
///
/// The picture's own grid is one column per pixel, and committing that
/// as text is hundreds of kilobytes a zoom that nobody could read. The
/// same children folded over the same window onto 256 columns is the
/// same fold at a resolution a diff can show, and it moves for every
/// change that is not confined to a single screen column — which the
/// picture beside it catches structurally.
pub const FIXTURE_COLUMNS: usize = 256;

/// Every folder's fold over one window, as the committed exact half of a
/// folder-item fixture (the #48 amendment).
#[must_use]
pub fn fixture_text(folders: &[Folder], take: usize, from: f64, to: f64) -> String {
    use core::fmt::Write as _;
    let mut out = format!("window {from:.6} {to:.6}\ntake {take}\n");
    for folder in folders {
        let _ = write!(
            out,
            "\nfolder {}\nchildren {}\ntakes {}\nrevision {:016x}\nmute-mask {:016x}\n",
            folder.name,
            folder.children.len(),
            folder.take_count,
            folder.revision(),
            folder.mute_mask(),
        );
        for child in &folder.children {
            let _ = writeln!(
                out,
                "  child {} role {:?} side {:?} muted {} hidden {} takes {}",
                child.name,
                child.role,
                child.side,
                child.muted,
                child.hidden,
                child.takes.len()
            );
        }
        out.push_str(
            &folder
                .fold(take, Grid::over(from, to, FIXTURE_COLUMNS))
                .to_text(),
        );
    }
    out
}

/// A folder, and the children its item is a view of.
///
/// The folder owns no peaks. Everything here is either its children's or
/// derived from them, which is why [`revision`](Self::revision) is
/// computed rather than incremented: there is no counter to forget to
/// bump.
#[derive(Clone, Debug)]
pub struct Folder {
    pub guid: String,
    pub name: String,
    /// The track colour the dim envelope and the sides are drawn in.
    pub colour: Color,
    /// Roles for a kit, sides for a Channel.
    pub group_by: GroupBy,
    pub children: Vec<Child>,
    /// Where the item starts, in project seconds — the earliest of the
    /// children's items for this take.
    pub start_secs: f64,
    /// How long it runs — to the latest of their ends.
    pub length_secs: f64,
    /// How many takes the children were recorded across.
    pub take_count: usize,
}

impl Folder {
    /// A bit per child, set when it is out of the sum. Children past 64
    /// share the top bit, which costs a redundant refold and never a
    /// wrong picture.
    #[must_use]
    pub fn mute_mask(&self) -> u64 {
        self.children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.muted)
            .fold(0_u64, |mask, (i, _)| {
                let bit = u32::try_from(i).unwrap_or(63).min(63);
                mask | 1_u64.checked_shl(bit).unwrap_or(1 << 63)
            })
    }

    /// The revision the cache invalidates on.
    ///
    /// Moves when a child's peaks change, when a child joins or leaves,
    /// and when a child is **muted** — a muted child leaves the sum,
    /// because the folder hears what the mix hears.
    ///
    /// It does **not** move when a child is **hidden**: hiding is the
    /// TCP's business, the sum is unchanged, and the picture replays
    /// from cache byte for byte.
    ///
    /// r[impl flow.drums.comping.folder-items]
    #[must_use]
    pub fn revision(&self) -> u64 {
        // FNV-1a over what the sum is made of. A hash rather than a
        // counter so that "the revision bumped" cannot be out of step
        // with "something changed" — there is nothing to remember to
        // call.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for child in &self.children {
            eat(child.guid.as_bytes());
            eat(&[u8::from(child.muted)]);
            for take in &child.takes {
                for placed in &take.placements {
                    eat(&placed.start_secs.to_bits().to_le_bytes());
                    eat(&placed.length_secs.to_bits().to_le_bytes());
                    eat(&placed.peaks.fingerprint().to_le_bytes());
                }
            }
        }
        hash
    }

    /// The grid the item folds onto at `pixels_per_sec`: the item's own
    /// extent, one column per pixel of it.
    ///
    /// One column per pixel is what every DAW draws and what makes the
    /// fold's grid the screen's grid. Past [`MAX_COLUMNS`] the picture
    /// is coarser than the screen rather than a scene of forty thousand
    /// fills.
    #[must_use]
    pub fn grid_at(&self, pixels_per_sec: f64) -> Grid {
        let wanted = self.length_secs * pixels_per_sec;
        let columns = if wanted.is_finite() && wanted > 1.0 {
            crate::num::index(wanted.round()).clamp(1, MAX_COLUMNS)
        } else {
            1
        };
        Grid::over(self.start_secs, self.start_secs + self.length_secs, columns)
    }

    /// Everything this folder's picture for `take` on `grid` depends on.
    ///
    /// The zoom rides in as the grid's **column count**, which is the
    /// quantity the fold actually turns on. Keying on the zoom itself
    /// would let two pixels-per-second inside one bucket share a
    /// picture while folding onto column counts dozens apart — for a
    /// long folder, a tenth of a pixel a second is dozens of columns.
    #[must_use]
    pub fn picture_key(&self, take: usize, grid: Grid) -> PictureKey {
        PictureKey {
            revision: self.revision(),
            mute_mask: self.mute_mask(),
            take,
            columns: grid.columns,
            group_by: self.group_by,
        }
    }

    /// Fold `take` onto `grid`.
    #[must_use]
    pub fn fold(&self, take: usize, grid: Grid) -> Fold {
        fold::fold(&self.children, take, grid, self.group_by)
    }

    /// Record the picture for `take` on `grid`, in the unit box.
    #[must_use]
    pub fn picture(&self, take: usize, grid: Grid) -> Scene {
        let mut scene = Scene::new();
        draw::draw(&mut scene, &self.fold(take, grid), self.colour);
        scene
    }

    /// Set a child's mute by guid, and say whether anything moved.
    pub fn set_muted(&mut self, guid: &str, muted: bool) -> bool {
        self.children
            .iter_mut()
            .find(|c| c.guid == guid)
            .is_some_and(|c| {
                let moved = c.muted != muted;
                c.muted = muted;
                moved
            })
    }

    /// Set a child's hidden flag by guid, and say whether anything
    /// moved. The sum is unchanged either way.
    pub fn set_hidden(&mut self, guid: &str, hidden: bool) -> bool {
        self.children
            .iter_mut()
            .find(|c| c.guid == guid)
            .is_some_and(|c| {
                let moved = c.hidden != hidden;
                c.hidden = hidden;
                moved
            })
    }
}

/// The folders on screen, and the pictures kept for them.
///
/// Held beside the arrangement rather than inside it: the arrangement
/// records its lanes once at one pixel per second and scales them, and a
/// fold cannot be recorded that way. This is the part of a row that has
/// to be rebuilt when the zoom bucket moves.
#[derive(Default)]
pub struct FolderItems {
    pub folders: Vec<Folder>,
    cache: PictureCache,
}

impl FolderItems {
    #[must_use]
    pub fn new(folders: Vec<Folder>) -> Self {
        Self {
            folders,
            cache: PictureCache::default(),
        }
    }

    /// Replays served, and pictures built.
    #[must_use]
    pub const fn counts(&self) -> (usize, usize) {
        self.cache.counts()
    }

    /// How many pictures are held.
    #[must_use]
    pub fn held(&self) -> usize {
        self.cache.len()
    }

    /// The picture for one folder's take, built if it is not held.
    pub fn picture(&mut self, at: usize, take: usize, pixels_per_sec: f64) -> Option<&Scene> {
        // Split borrow: the folder is read out of one field while the
        // picture is written into another, which is what lets the
        // painter replay straight out of the cache with no clone.
        let Self { folders, cache } = self;
        held(folders, cache, at, take, pixels_per_sec)
    }

    /// Draw one folder's take at `place`, replaying the held picture
    /// when nothing it depends on has moved.
    ///
    /// r[impl flow.drums.comping.folder-items]
    pub fn paint(
        &mut self,
        painter: &mut impl PaintScene,
        at: usize,
        take: usize,
        place: Place,
        pixels_per_sec: f64,
    ) {
        let Self { folders, cache } = self;
        if let Some(scene) = held(folders, cache, at, take, pixels_per_sec) {
            // Recorded in the unit box, so the replay is one transform.
            draw::replay(painter, scene, place);
        }
    }
}

/// The held picture for one folder's take at one zoom, built into the
/// cache when it is not already there.
///
/// Free rather than a method so both callers can take the folders and
/// the cache as separate borrows: a method on `&mut self` would borrow
/// the whole struct and force the painter to clone the scene it is about
/// to replay.
fn held<'a>(
    folders: &'a [Folder],
    cache: &'a mut PictureCache,
    at: usize,
    take: usize,
    pixels_per_sec: f64,
) -> Option<&'a Scene> {
    let folder = folders.get(at)?;
    let grid = folder.grid_at(pixels_per_sec);
    let key = folder.picture_key(take, grid);
    cache.get_or_build(key, || folder.picture(take, grid))
}

#[cfg(test)]
mod tests;
