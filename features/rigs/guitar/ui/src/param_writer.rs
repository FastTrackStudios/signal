//! Parameter writes to the rig, coalesced.
//!
//! A drag produces an edit per pointer event, per field — an EQ node moves
//! frequency and gain at once, sixty times a second or more. Sent as one
//! spawned RPC each, a drag filled vox's 64 in-flight requests, and the
//! server then closes the connection on the 65th ("max_concurrent_requests
//! exceeded") — the "vox error" that dropped the rig mid-drag.
//!
//! So every parameter write goes through one [`ParamWriter`]: the newest
//! value per (block, param) replaces any older one still waiting, at most
//! [`MAX_IN_FLIGHT`] writes are on the wire at once, and one parameter never
//! has two — its writes cannot arrive out of order. A drag sends as fast as
//! the link answers and no faster, and always lands on where the pointer
//! stopped.
//!
//! The queue ([`Coalescer`]) is plain data; the writer takes how to send and
//! how to spawn, so tests drive it on tokio (native) and on wasm without a
//! Dioxus runtime.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// One parameter write: `(block id, param, value in the DSP's units)`.
pub type Edit = (String, String, f32);

/// Writes allowed on the wire at once — well under vox's 64.
pub const MAX_IN_FLIGHT: usize = 8;

/// The waiting writes, newest value per parameter.
#[derive(Default, Debug)]
pub struct Coalescer {
    pending: Vec<Edit>,
    in_flight: HashSet<(String, String)>,
}

impl Coalescer {
    /// Queue `value` for `(block, param)`, replacing a waiting one.
    pub fn push(&mut self, block: String, param: String, value: f32) {
        match self
            .pending
            .iter_mut()
            .find(|(b, p, _)| *b == block && *p == param)
        {
            Some(e) => e.2 = value,
            None => self.pending.push((block, param, value)),
        }
    }

    /// The oldest waiting write whose parameter is not already on the wire;
    /// it is on the wire from now until [`done`](Self::done).
    pub fn next(&mut self) -> Option<Edit> {
        let i = self
            .pending
            .iter()
            .position(|(b, p, _)| !self.in_flight.contains(&(b.clone(), p.clone())))?;
        let e = self.pending.remove(i);
        self.in_flight.insert((e.0.clone(), e.1.clone()));
        Some(e)
    }

    /// The write for `(block, param)` came back.
    pub fn done(&mut self, block: &str, param: &str) {
        self.in_flight
            .remove(&(block.to_string(), param.to_string()));
    }

    #[must_use]
    pub fn waiting(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn on_the_wire(&self) -> usize {
        self.in_flight.len()
    }
}

type Send = Rc<dyn Fn(Edit) -> Pin<Box<dyn Future<Output = ()>>>>;
type Spawn = Rc<dyn Fn(Pin<Box<dyn Future<Output = ()>>>)>;

/// See the module docs. Cheap to clone (all clones share one queue).
#[derive(Clone)]
pub struct ParamWriter {
    queue: Rc<RefCell<Coalescer>>,
    workers: Rc<Cell<usize>>,
    send: Send,
    spawn: Spawn,
}

impl ParamWriter {
    /// `send` makes one write and resolves when it is answered; `spawn` runs
    /// a sender task.
    pub fn new(
        send: impl Fn(Edit) -> Pin<Box<dyn Future<Output = ()>>> + 'static,
        spawn: impl Fn(Pin<Box<dyn Future<Output = ()>>>) + 'static,
    ) -> Self {
        Self {
            queue: Rc::default(),
            workers: Rc::default(),
            send: Rc::new(send),
            spawn: Rc::new(spawn),
        }
    }

    /// Write `value` to `param` of block `block` — soon, and only the newest.
    pub fn set(&self, block: impl Into<String>, param: impl Into<String>, value: f32) {
        self.queue
            .borrow_mut()
            .push(block.into(), param.into(), value);
        while self.workers.get() < MAX_IN_FLIGHT && self.has_sendable() {
            self.workers.set(self.workers.get() + 1);
            let me = self.clone();
            (self.spawn)(Box::pin(async move { me.work().await }));
        }
    }

    fn has_sendable(&self) -> bool {
        let q = self.queue.borrow();
        q.pending
            .iter()
            .any(|(b, p, _)| !q.in_flight.contains(&(b.clone(), p.clone())))
    }

    async fn work(self) {
        loop {
            let next = self.queue.borrow_mut().next();
            let Some((block, param, value)) = next else {
                break;
            };
            (self.send)((block.clone(), param.clone(), value)).await;
            self.queue.borrow_mut().done(&block, &param);
        }
        self.workers.set(self.workers.get() - 1);
    }

    /// Writes waiting plus writes on the wire — zero once everything landed.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        let q = self.queue.borrow();
        q.waiting() + q.on_the_wire()
    }
}

/// Provide the component tree's [`ParamWriter`], sending over `rig`. Call
/// once, at the root that owns the rig client.
pub fn use_param_writer(rig: Option<signal_guitar_proto::rig::RigClient>) -> ParamWriter {
    use dioxus::prelude::*;
    use_context_provider(move || {
        ParamWriter::new(
            move |(block, param, value): Edit| {
                let rig = rig.clone();
                Box::pin(async move {
                    if let Some(r) = rig {
                        let _ = r.set_block_param(block, param, value).await;
                    }
                })
            },
            |task| {
                spawn(task);
            },
        )
    })
}

/// `write_param` on the rig client: the same shape as `set_block_param`, but
/// through the tree's [`ParamWriter`] — so a call site that awaited the RPC
/// now queues the write and moves on.
pub trait WriteParam {
    fn write_param(&self, block: String, param: String, value: f32) -> std::future::Ready<()>;
}

impl WriteParam for signal_guitar_proto::rig::RigClient {
    fn write_param(&self, block: String, param: String, value: f32) -> std::future::Ready<()> {
        match dioxus::prelude::try_consume_context::<ParamWriter>() {
            Some(w) => w.set(block, param, value),
            // No writer above this component (a test mounting it alone):
            // one direct write.
            None => {
                let r = self.clone();
                dioxus::prelude::spawn(async move {
                    let _ = r.set_block_param(block, param, value).await;
                });
            }
        }
        std::future::ready(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn the_newest_value_replaces_a_waiting_one() {
        let mut c = Coalescer::default();
        for i in 0..1000 {
            c.push("eq".into(), "b1_freq".into(), i as f32);
            c.push("eq".into(), "b1_gain".into(), -(i as f32));
        }
        assert_eq!(c.waiting(), 2);
        assert_eq!(c.next(), Some(("eq".into(), "b1_freq".into(), 999.0)));
        assert_eq!(c.next(), Some(("eq".into(), "b1_gain".into(), -999.0)));
        assert_eq!(c.next(), None);
    }

    #[test]
    fn a_parameter_is_never_on_the_wire_twice() {
        let mut c = Coalescer::default();
        c.push("eq".into(), "b1_freq".into(), 1.0);
        assert!(c.next().is_some());
        c.push("eq".into(), "b1_freq".into(), 2.0);
        assert_eq!(c.next(), None, "the first write has not come back");
        c.done("eq", "b1_freq");
        assert_eq!(c.next(), Some(("eq".into(), "b1_freq".into(), 2.0)));
    }

    /// A hard drag on every band of an EQ: nothing is dropped that matters
    /// (each parameter ends on its last value), and the wire never holds
    /// more than the cap — driven by a hand-cranked executor, so the same
    /// test runs on wasm.
    #[test]
    fn a_drag_flood_stays_under_the_cap_and_lands_on_the_last_value() {
        use std::collections::HashMap;
        use std::task::{Context, Poll, Waker};

        type Task = Pin<Box<dyn Future<Output = ()>>>;
        let tasks: Rc<RefCell<Vec<Task>>> = Rc::default();
        let landed: Rc<RefCell<HashMap<String, f32>>> = Rc::default();
        let peak = Rc::new(Cell::new(0usize));
        let live = Rc::new(Cell::new(0usize));

        // A write that takes a couple of polls to "come back".
        struct Wire(u8);
        impl Future for Wire {
            type Output = ();
            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                if self.0 == 0 {
                    Poll::Ready(())
                } else {
                    self.0 -= 1;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
        let writer = {
            let (landed, peak, live) = (landed.clone(), peak.clone(), live.clone());
            let tasks = tasks.clone();
            ParamWriter::new(
                move |(b, p, v): Edit| {
                    let (landed, peak, live) = (landed.clone(), peak.clone(), live.clone());
                    live.set(live.get() + 1);
                    peak.set(peak.get().max(live.get()));
                    Box::pin(async move {
                        Wire(2).await;
                        landed.borrow_mut().insert(format!("{b}/{p}"), v);
                        live.set(live.get() - 1);
                    })
                },
                move |t| tasks.borrow_mut().push(t),
            )
        };
        let run = |tasks: &Rc<RefCell<Vec<Task>>>| {
            let mut cx = Context::from_waker(Waker::noop());
            let mut ts = std::mem::take(&mut *tasks.borrow_mut());
            ts.retain_mut(|t| t.as_mut().poll(&mut cx).is_pending());
            tasks.borrow_mut().extend(ts);
        };

        for step in 0..3000 {
            let band = step % 8 + 1;
            writer.set("eq", format!("b{band}_freq"), step as f32);
            writer.set("eq", format!("b{band}_gain"), -(step as f32));
            if step % 3 == 0 {
                run(&tasks);
            }
        }
        while writer.outstanding() > 0 {
            run(&tasks);
        }
        assert!(
            peak.get() <= MAX_IN_FLIGHT,
            "peak {} on the wire",
            peak.get()
        );
        let landed = landed.borrow();
        for band in 1..=8 {
            let last = (0..3000).rev().find(|s| s % 8 + 1 == band).unwrap() as f32;
            assert_eq!(landed[&format!("eq/b{band}_freq")], last);
            assert_eq!(landed[&format!("eq/b{band}_gain")], -last);
        }
    }
}
