//! A file that arrives in pieces: the bytes of a proxy fetched from
//! somewhere else (Task, a peer), readable where they have landed.
//!
//! [`SparseBytes`] holds what has arrived — in memory, or in a file on disk
//! so a long set does not sit in RAM — and which ranges those are. In
//! memory it is held in blocks allocated as bytes arrive, never the whole
//! file up front, and a holder that must stay small (a browser) lets go of
//! what is far from where it plays ([`SparseBytes::keep_only`]): those
//! bytes are simply missing again, and come back the way they came. A reader
//! over it ([`SparseView`]) reads what is there and, where it is not, says
//! **would block** and records the range it wanted: the decoder stops, the
//! track is silent there for now, and the fetcher knows what to bring next.
//! Nothing ever waits on the network.
//!
//! A view is the stream's header pages followed by the file from a given
//! page on — what [`crate::ogg_stream::OggStream::open_view`] decodes, so a
//! seek into a remote proxy needs only the bytes from the page before the
//! frame asked for (found by [`crate::ogg_index::OggIndex`]), never a
//! bisection across the whole file.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::sync::{Arc, Mutex};

/// The size of a block of a file held in memory.
const BLOCK: u64 = 64 * 1024;

/// Where the arrived bytes are kept.
enum Store {
    /// Blocks by index, each [`BLOCK`] bytes (the last one shorter),
    /// allocated when a byte of it arrives.
    Memory(HashMap<u64, Box<[u8]>>),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::fs::File),
}

struct Inner {
    store: Store,
    /// The ranges that have arrived: sorted, disjoint, not touching.
    have: Vec<Range<u64>>,
    /// The last range a reader asked for and found missing.
    wanted: Option<Range<u64>>,
    /// How many times bytes have arrived: see [`SparseBytes::arrivals`].
    arrivals: u64,
}

/// A file's bytes, as they arrive.
pub struct SparseBytes {
    len: u64,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for SparseBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SparseBytes")
            .field("len", &self.len)
            .field("have", &self.have())
            .finish()
    }
}

fn lock(m: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl SparseBytes {
    /// A `len`-byte file kept in memory.
    #[must_use]
    pub fn in_memory(len: u64) -> Arc<Self> {
        Arc::new(Self {
            len,
            inner: Mutex::new(Inner {
                store: Store::Memory(HashMap::new()),
                have: Vec::new(),
                wanted: None,
                arrivals: 0,
            }),
        })
    }

    /// The bytes of the block `index`'s span.
    fn block_span(&self, index: u64) -> Range<u64> {
        let start = index.saturating_mul(BLOCK);
        start..start.saturating_add(BLOCK).min(self.len)
    }

    /// A `len`-byte file kept in a file at `path` (created, sized; its
    /// contents are what has arrived).
    ///
    /// # Errors
    ///
    /// The file cannot be created.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn on_disk(len: u64, path: &std::path::Path) -> std::io::Result<Arc<Self>> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(len)?;
        Ok(Arc::new(Self {
            len,
            inner: Mutex::new(Inner {
                store: Store::File(file),
                have: Vec::new(),
                wanted: None,
                arrivals: 0,
            }),
        }))
    }

    /// The whole file's length.
    #[must_use]
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Whether the file is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Bytes that arrived at `offset`.
    ///
    /// # Errors
    ///
    /// Writing to the on-disk store failed.
    pub fn insert(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        let end = offset
            .saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX))
            .min(self.len);
        if end <= offset {
            return Ok(());
        }
        let mut inner = lock(&self.inner);
        match &mut inner.store {
            Store::Memory(blocks) => {
                let mut at = offset;
                while at < end {
                    let index = at / BLOCK;
                    let span = self.block_span(index);
                    let upto = span.end.min(end);
                    let block = blocks.entry(index).or_insert_with(|| {
                        vec![0; usize::try_from(span.end - span.start).unwrap_or(0)]
                            .into_boxed_slice()
                    });
                    let (from, to) = (to_usize(at - span.start), to_usize(upto - span.start));
                    let (src_from, src_to) = (to_usize(at - offset), to_usize(upto - offset));
                    if let (Some(dst), Some(src)) =
                        (block.get_mut(from..to), data.get(src_from..src_to))
                    {
                        dst.copy_from_slice(src);
                    }
                    at = upto;
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Store::File(file) => {
                file.seek(SeekFrom::Start(offset))?;
                let take = usize::try_from(end - offset).unwrap_or(0);
                std::io::Write::write_all(file, data.get(..take).unwrap_or_default())?;
            }
        }
        add_range(&mut inner.have, offset..end);
        inner.arrivals += 1;
        if inner
            .wanted
            .as_ref()
            .is_some_and(|w| covered(&inner.have, w))
        {
            inner.wanted = None;
        }
        Ok(())
    }

    /// How many times bytes have arrived. Unchanged means nothing new
    /// has: a reader that stopped short can wait for it to move rather
    /// than read the same bytes again to find out.
    #[must_use]
    pub fn arrivals(&self) -> u64 {
        lock(&self.inner).arrivals
    }

    /// The ranges that have arrived.
    #[must_use]
    pub fn have(&self) -> Vec<Range<u64>> {
        lock(&self.inner).have.clone()
    }

    /// Whether all of `range` has arrived.
    #[must_use]
    pub fn has(&self, range: &Range<u64>) -> bool {
        covered(
            &lock(&self.inner).have,
            &(range.start..range.end.min(self.len)),
        )
    }

    /// What of `range` has not arrived yet.
    #[must_use]
    pub fn missing(&self, range: &Range<u64>) -> Vec<Range<u64>> {
        let range = range.start..range.end.min(self.len);
        let inner = lock(&self.inner);
        let mut out = Vec::new();
        let mut at = range.start;
        for have in inner
            .have
            .iter()
            .filter(|h| h.end > range.start && h.start < range.end)
        {
            if have.start > at {
                out.push(at..have.start);
            }
            at = at.max(have.end);
        }
        if at < range.end {
            out.push(at..range.end);
        }
        out
    }

    /// The last range a reader wanted and did not find (cleared once it
    /// arrives) — what a fetcher brings next.
    #[must_use]
    pub fn wanted(&self) -> Option<Range<u64>> {
        lock(&self.inner).wanted.clone()
    }

    /// Read into `buf` from `offset`, as much as has arrived contiguously;
    /// `None` (and `wanted` set) when the first byte has not.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<Option<usize>> {
        if offset >= self.len {
            return Ok(Some(0));
        }
        let mut inner = lock(&self.inner);
        let Some(have) = inner
            .have
            .iter()
            .find(|h| h.start <= offset && offset < h.end)
            .cloned()
        else {
            let want = offset
                ..offset
                    .saturating_add(u64::try_from(buf.len()).unwrap_or(0))
                    .min(self.len);
            inner.wanted = Some(want);
            return Ok(None);
        };
        let n =
            usize::try_from((have.end - offset).min(u64::try_from(buf.len()).unwrap_or(u64::MAX)))
                .unwrap_or(0);
        match &mut inner.store {
            Store::Memory(blocks) => {
                let end = offset + u64::try_from(n).unwrap_or(0);
                let mut at = offset;
                while at < end {
                    let index = at / BLOCK;
                    let span = self.block_span(index);
                    let upto = span.end.min(end);
                    let (from, to) = (to_usize(at - span.start), to_usize(upto - span.start));
                    let (dst_from, dst_to) = (to_usize(at - offset), to_usize(upto - offset));
                    if let (Some(src), Some(dst)) = (
                        blocks.get(&index).and_then(|b| b.get(from..to)),
                        buf.get_mut(dst_from..dst_to),
                    ) {
                        dst.copy_from_slice(src);
                    }
                    at = upto;
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Store::File(file) => {
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(buf.get_mut(..n).unwrap_or_default())?;
            }
        }
        Ok(Some(n))
    }
}

impl SparseBytes {
    /// Let go of everything held in memory outside `keep` (whole blocks: a
    /// block touching `keep` stays). What is let go is missing again — a
    /// reader wanting it says so, and a fetcher brings it back. On disk
    /// nothing is let go: a file costs no memory.
    pub fn keep_only(&self, keep: &[Range<u64>]) {
        let mut inner = lock(&self.inner);
        let Store::Memory(blocks) = &mut inner.store else {
            return;
        };
        blocks.retain(|index, _| {
            let span = self.block_span(*index);
            keep.iter()
                .any(|k| k.start < span.end && span.start < k.end)
        });
        let mut kept: Vec<u64> = blocks.keys().copied().collect();
        kept.sort_unstable();
        let mut spans: Vec<Range<u64>> = Vec::new();
        for index in kept {
            let span = self.block_span(index);
            match spans.last_mut() {
                Some(last) if last.end == span.start => last.end = span.end,
                _ => spans.push(span),
            }
        }
        let have = std::mem::take(&mut inner.have);
        for h in have {
            for s in &spans {
                let (start, end) = (h.start.max(s.start), h.end.min(s.end));
                if start < end {
                    inner.have.push(start..end);
                }
            }
        }
    }

    /// Bytes held in memory now.
    #[must_use]
    pub fn resident(&self) -> u64 {
        match &lock(&self.inner).store {
            Store::Memory(blocks) => blocks
                .values()
                .map(|b| u64::try_from(b.len()).unwrap_or(0))
                .sum(),
            #[cfg(not(target_arch = "wasm32"))]
            Store::File(_) => 0,
        }
    }
}

fn to_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn covered(have: &[Range<u64>], range: &Range<u64>) -> bool {
    range.is_empty()
        || have
            .iter()
            .any(|h| h.start <= range.start && range.end <= h.end)
}

/// Add `r` to a sorted, disjoint set, merging what it touches.
fn add_range(set: &mut Vec<Range<u64>>, r: Range<u64>) {
    let mut merged = r;
    set.retain(|h| {
        if h.end < merged.start || h.start > merged.end {
            true
        } else {
            merged = merged.start.min(h.start)..merged.end.max(h.end);
            false
        }
    });
    let at = set.partition_point(|h| h.start < merged.start);
    set.insert(at, merged);
}

/// A reader over a stream's header pages followed by the file from
/// `from` on — positions `0..header.len()` are the header, then the file.
pub struct SparseView {
    bytes: Arc<SparseBytes>,
    header: Range<u64>,
    from: u64,
    pos: u64,
}

impl SparseView {
    /// The header (`header`, bytes of the file) then the file from `from`.
    #[must_use]
    pub fn new(bytes: Arc<SparseBytes>, header: Range<u64>, from: u64) -> Self {
        Self {
            bytes,
            header,
            from,
            pos: 0,
        }
    }

    /// The whole file, from its start.
    #[must_use]
    pub fn whole(bytes: Arc<SparseBytes>) -> Self {
        Self {
            bytes,
            header: 0..0,
            from: 0,
            pos: 0,
        }
    }

    fn header_len(&self) -> u64 {
        self.header.end - self.header.start
    }

    fn view_len(&self) -> u64 {
        self.header_len() + self.bytes.len().saturating_sub(self.from)
    }

    /// Where view position `pos` is in the file.
    fn file_offset(&self, pos: u64) -> u64 {
        let h = self.header_len();
        if pos < h {
            self.header.start + pos
        } else {
            self.from + (pos - h)
        }
    }
}

impl Read for SparseView {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let h = self.header_len();
        // Never read across the header's end in one go: the file continues
        // elsewhere after it.
        let room = if self.pos < h { h - self.pos } else { u64::MAX };
        let n = usize::try_from(room).unwrap_or(usize::MAX).min(buf.len());
        let offset = self.file_offset(self.pos);
        match self
            .bytes
            .read_at(offset, buf.get_mut(..n).unwrap_or_default())?
        {
            Some(got) => {
                self.pos += u64::try_from(got).unwrap_or(0);
                Ok(got)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "not arrived yet",
            )),
        }
    }
}

impl Seek for SparseView {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let len = i128::from(self.view_len());
        let at = match to {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => len + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        self.pos = u64::try_from(at.clamp(0, len)).unwrap_or(0);
        Ok(self.pos)
    }
}

impl symphonia_core::io::MediaSource for SparseView {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.view_len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_has_arrived_reads_and_what_has_not_would_block() {
        let bytes = SparseBytes::in_memory(100);
        bytes.insert(10, &[7; 20]).unwrap();
        let mut view = SparseView::whole(Arc::clone(&bytes));
        view.seek(SeekFrom::Start(12)).unwrap();
        let mut buf = [0u8; 50];
        assert_eq!(
            view.read(&mut buf).unwrap(),
            18,
            "up to the end of what arrived"
        );
        let e = view.read(&mut buf).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::WouldBlock);
        assert_eq!(bytes.wanted(), Some(30..80));
        bytes.insert(30, &[9; 50]).unwrap();
        assert_eq!(bytes.wanted(), None, "cleared once it arrives");
        assert_eq!(view.read(&mut buf).unwrap(), 50);
        assert_eq!(bytes.have(), vec![10..80]);
        assert_eq!(bytes.missing(&(0..100)), vec![0..10, 80..100]);
    }

    #[test]
    fn a_view_is_the_header_then_the_file_from_a_page() {
        let bytes = SparseBytes::in_memory(10);
        bytes.insert(0, b"HHabcdefgh").unwrap();
        let mut view = SparseView::new(bytes, 0..2, 6);
        let mut out = Vec::new();
        view.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"HHefgh");
    }

    #[test]
    fn memory_is_held_in_blocks_and_what_is_let_go_is_missing_again() {
        let len = BLOCK * 10 + 100;
        let bytes = SparseBytes::in_memory(len);
        assert_eq!(bytes.resident(), 0, "nothing allocated up front");
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        // Across block edges, in two pieces.
        bytes
            .insert(
                BLOCK - 10,
                &data[(BLOCK - 10) as usize..(3 * BLOCK + 5) as usize],
            )
            .unwrap();
        bytes
            .insert(9 * BLOCK, &data[(9 * BLOCK) as usize..])
            .unwrap();
        assert_eq!(
            bytes.resident(),
            4 * BLOCK + (BLOCK + 100),
            "only the blocks touched"
        );
        let mut view = SparseView::whole(Arc::clone(&bytes));
        view.seek(SeekFrom::Start(BLOCK - 10)).unwrap();
        let mut got = vec![0u8; (2 * BLOCK + 15) as usize];
        view.read_exact(&mut got).unwrap();
        assert_eq!(got, &data[(BLOCK - 10) as usize..(3 * BLOCK + 5) as usize]);

        // Keep the end: the front is gone, and missing again.
        bytes.keep_only(&[9 * BLOCK..len]);
        assert_eq!(bytes.resident(), BLOCK + 100);
        assert_eq!(bytes.have(), vec![9 * BLOCK..len]);
        view.seek(SeekFrom::Start(BLOCK)).unwrap();
        assert_eq!(
            view.read(&mut got).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        // And it comes back the way it came.
        bytes
            .insert(BLOCK, &data[BLOCK as usize..(2 * BLOCK) as usize])
            .unwrap();
        view.seek(SeekFrom::Start(BLOCK)).unwrap();
        let mut again = vec![0u8; BLOCK as usize];
        view.read_exact(&mut again).unwrap();
        assert_eq!(again, &data[BLOCK as usize..(2 * BLOCK) as usize]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn on_disk_holds_what_arrived() {
        let path = std::env::temp_dir().join(format!("fts-sparse-{}.bin", std::process::id()));
        let bytes = SparseBytes::on_disk(64, &path).unwrap();
        bytes.insert(32, &[5; 16]).unwrap();
        let mut view = SparseView::whole(Arc::clone(&bytes));
        view.seek(SeekFrom::Start(40)).unwrap();
        let mut buf = [0u8; 4];
        view.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [5; 4]);
        let _ = std::fs::remove_file(path);
    }
}
