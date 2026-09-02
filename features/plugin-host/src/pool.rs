//! Many instances of one plugin, the way a DAW holds them.
//!
//! A mix with a hundred compressors on it is unremarkable, and no DAW thinks
//! twice about it. Signal's host could only ever be asked for one instance at
//! a time, which made the obvious question — *can we carry a real session?* —
//! unanswerable without writing the fan-out by hand at every call site.
//!
//! Two facts shape the design, and they pull in opposite directions:
//!
//! - **Instances are cheap; bundles are not.** A `.clap` bundle is `dlopen`ed
//!   once and cached process-wide for the life of the process (a CLAP library
//!   is never unloaded — plugins register thread-local destructors, and
//!   unmapping the code under them crashes at thread exit). So the hundredth
//!   instance costs a fraction of the first, and pooling is mostly about not
//!   paying the bundle cost repeatedly.
//! - **Creation is not processing.** CLAP asks that the plugin factory be
//!   entered from one thread at a time, so [`PluginPool::open`] can be told to
//!   serialise creation. Once created, instances are independent
//!   [`PluginInstance`]s — the trait is `Send` — and rendering them across
//!   threads is exactly what an audio engine does.
//!
//! The pool therefore separates the two: it owns creation, and hands the
//! instances out for the caller to schedule however it likes.
//!
//! ```no_run
//! use signal_plugin_host::PluginPool;
//!
//! let pool = PluginPool::open("FTS Comp.clap".into(), 100, 48_000.0, 512, true)?;
//! let mut instances = pool.into_instances();
//! // …render across as many threads as you like; each instance is independent.
//! # Ok::<(), signal_plugin_host::PluginError>(())
//! ```

use std::path::PathBuf;

use crate::{HostedPlugin, PluginDescriptor, PluginError};

/// A set of independent instances of one plugin, all prepared and ready.
pub struct PluginPool {
    descriptor: PluginDescriptor,
    instances: Vec<HostedPlugin>,
}

impl PluginPool {
    /// Create `count` instances of the plugin at `path`, each prepared at
    /// `sample_rate` / `block_size`.
    ///
    /// `parallel` creates them from several threads. That is safe for the
    /// backends here — the bundle cache is behind a mutex and instantiation
    /// touches no shared plugin state — but it is a switch rather than the
    /// only behaviour, because a plugin that misbehaves under concurrent
    /// creation is a thing that exists, and `--load serial` is then the
    /// difference between a measurement and a crash.
    ///
    /// Returns [`PluginError::UnsupportedFormat`] if `path` resolves to the
    /// synthetic backend, which has no bundle to instantiate from — build
    /// those with [`PluginPool::build`] instead.
    pub fn open(
        path: PathBuf,
        count: usize,
        sample_rate: f64,
        block_size: u32,
        parallel: bool,
    ) -> Result<Self, PluginError> {
        Self::build(count, parallel, move |_| {
            let mut plugin = HostedPlugin::load(&path)?
                .ok_or(PluginError::UnsupportedFormat(crate::PluginFormat::Synthetic))?;
            plugin.prepare(sample_rate, block_size)?;
            Ok(plugin)
        })
    }

    /// Build a pool from an arbitrary constructor.
    ///
    /// This is the seam that lets the pool be tested with nothing installed,
    /// and it is also how a caller pools *synthetic* instances — Signal's own
    /// `signal-fx` processors, which have no bundle on disk.
    ///
    /// The first error stops the build and is returned; a partially built pool
    /// is dropped rather than handed back, because a pool that quietly holds
    /// fewer instances than asked for is how a load test comes to measure the
    /// wrong thing.
    pub fn build<F>(count: usize, parallel: bool, make: F) -> Result<Self, PluginError>
    where
        F: Fn(usize) -> Result<HostedPlugin, PluginError> + Sync,
    {
        if count == 0 {
            return Err(PluginError::LoadFailed("a pool of zero instances".into()));
        }

        let instances: Vec<HostedPlugin> = if parallel {
            let threads = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(count);
            let per = count.div_ceil(threads);
            let make = &make;

            let mut collected: Vec<(usize, Result<HostedPlugin, PluginError>)> =
                std::thread::scope(|s| {
                    let handles: Vec<_> = (0..threads)
                        .map(|t| {
                            let lo = t * per;
                            let hi = ((t + 1) * per).min(count);
                            s.spawn(move || {
                                (lo..hi).map(|i| (i, make(i))).collect::<Vec<_>>()
                            })
                        })
                        .collect();
                    handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
                });

            // Restore request order — threads finish out of order, and an
            // instance's index is how a caller correlates it with whatever it
            // was configured from.
            collected.sort_by_key(|(i, _)| *i);
            collected.into_iter().map(|(_, r)| r).collect::<Result<Vec<_>, _>>()?
        } else {
            (0..count).map(&make).collect::<Result<Vec<_>, _>>()?
        };

        let descriptor = instances
            .first()
            .map(|p| p.descriptor().clone())
            .ok_or_else(|| PluginError::LoadFailed("pool built no instances".into()))?;

        Ok(Self { descriptor, instances })
    }

    /// What every instance in this pool is.
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// The instances, for rendering in place.
    pub fn instances_mut(&mut self) -> &mut [HostedPlugin] {
        &mut self.instances
    }

    /// Take the instances out, to hand to worker threads.
    pub fn into_instances(self) -> Vec<HostedPlugin> {
        self.instances
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PluginEvents, PluginFormat, PluginInstance, PluginParamInfo};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A stub instance: applies a fixed gain so a render can be checked, and
    /// counts how many were built. No bundle, no disk, so these tests run
    /// anywhere — which is the same contract `signal-analyzer` keeps.
    struct Stub {
        gain: f32,
        prepared: bool,
    }

    impl PluginInstance for Stub {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "test.stub".into(),
                name: "Stub".into(),
                vendor: "tests".into(),
                version: "0".into(),
                format: PluginFormat::Synthetic,
            }
        }
        fn params(&mut self) -> Vec<PluginParamInfo> {
            Vec::new()
        }
        fn param_value(&mut self, _id: u32) -> Option<f64> {
            None
        }
        fn value_to_text(&mut self, _id: u32, _v: f64) -> Option<String> {
            None
        }
        fn text_to_value(&mut self, _id: u32, _t: &str) -> Option<f64> {
            None
        }
        fn latency(&mut self) -> u32 {
            0
        }
        fn prepare(&mut self, _sr: f64, _bs: u32) -> Result<(), PluginError> {
            self.prepared = true;
            Ok(())
        }
        fn is_prepared(&self) -> bool {
            self.prepared
        }
        fn process_block(
            &mut self,
            in_l: &[f32],
            in_r: &[f32],
            out_l: &mut [f32],
            out_r: &mut [f32],
            _events: &PluginEvents<'_>,
        ) -> Result<(), PluginError> {
            for i in 0..out_l.len().min(in_l.len()) {
                out_l[i] = in_l[i] * self.gain;
            }
            for i in 0..out_r.len().min(in_r.len()) {
                out_r[i] = in_r[i] * self.gain;
            }
            Ok(())
        }
        fn deactivate(&mut self) {}
    }

    /// Build a pool of stubs, reporting how many times the constructor ran.
    /// The counter is per-call, not a global: these tests run concurrently
    /// with each other, and a shared counter races between them.
    fn stub_pool_counted(
        count: usize,
        parallel: bool,
    ) -> (Result<PluginPool, PluginError>, usize) {
        let built = AtomicUsize::new(0);
        let pool = PluginPool::build(count, parallel, |_| {
            built.fetch_add(1, Ordering::SeqCst);
            let mut p = HostedPlugin::from_instance(Box::new(Stub { gain: 0.5, prepared: false }));
            p.prepare(48_000.0, 512)?;
            Ok(p)
        });
        let n = built.load(Ordering::SeqCst);
        (pool, n)
    }

    fn stub_pool(count: usize, parallel: bool) -> Result<PluginPool, PluginError> {
        stub_pool_counted(count, parallel).0
    }

    #[test]
    fn builds_the_requested_number_of_instances() {
        for parallel in [false, true] {
            let (pool, built) = stub_pool_counted(100, parallel);
            let pool = pool.unwrap();
            assert_eq!(pool.len(), 100, "parallel={parallel}");
            assert_eq!(built, 100, "constructor ran the wrong number of times");
            assert_eq!(pool.descriptor().name, "Stub");
            assert!(!pool.is_empty());
        }
    }

    #[test]
    fn a_count_not_divisible_by_the_thread_count_still_builds_exactly_that_many() {
        // The chunking is `div_ceil`, so the last thread's range is short —
        // an off-by-one here silently over- or under-builds the pool.
        for count in [1, 3, 7, 13, 31, 97] {
            let pool = stub_pool(count, true).unwrap();
            assert_eq!(pool.len(), count, "count={count}");
        }
    }

    #[test]
    fn asking_for_none_is_an_error_rather_than_an_empty_pool() {
        assert!(stub_pool(0, true).is_err());
        assert!(stub_pool(0, false).is_err());
    }

    #[test]
    fn a_failing_constructor_surfaces_rather_than_yielding_a_short_pool() {
        let r = PluginPool::build(16, true, |i| {
            if i == 9 {
                return Err(PluginError::UnsupportedFormat(PluginFormat::Synthetic));
            }
            Ok(HostedPlugin::from_instance(Box::new(Stub { gain: 1.0, prepared: false })))
        });
        assert!(r.is_err(), "one bad instance must fail the whole pool");
    }

    #[test]
    fn every_instance_is_prepared_and_processes_independently() {
        let mut pool = stub_pool(8, true).unwrap();
        let mut outputs = Vec::new();
        for inst in pool.instances_mut() {
            assert!(inst.is_prepared());
            let mut buf = vec![1.0f32; 64];
            inst.process_interleaved(&mut buf, &[], &[]).unwrap();
            outputs.push(buf[0]);
        }
        // Stub applies 0.5; every instance must have done so on its own buffer.
        assert!(outputs.iter().all(|v| (*v - 0.5).abs() < 1e-6), "{outputs:?}");
    }

    #[test]
    fn instances_render_correctly_across_threads() {
        let pool = stub_pool(64, true).unwrap();
        let mut instances = pool.into_instances();
        let bad = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for group in instances.chunks_mut(8) {
                let bad = &bad;
                s.spawn(move || {
                    for inst in group {
                        let mut buf = vec![1.0f32; 128];
                        if inst.process_interleaved(&mut buf, &[], &[]).is_err()
                            || buf.iter().any(|v| (*v - 0.5).abs() > 1e-6)
                        {
                            bad.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        assert_eq!(bad.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn instances_come_back_in_the_order_they_were_requested() {
        // Parallel construction finishes out of order; the pool must not.
        let pool = PluginPool::build(50, true, |i| {
            let mut p = HostedPlugin::from_instance(Box::new(Stub {
                gain: i as f32,
                prepared: false,
            }));
            p.prepare(48_000.0, 512)?;
            Ok(p)
        })
        .unwrap();
        for (i, inst) in pool.into_instances().into_iter().enumerate() {
            let mut inst = inst;
            let mut buf = vec![1.0f32; 2];
            inst.process_interleaved(&mut buf, &[], &[]).unwrap();
            assert_eq!(buf[0], i as f32, "instance {i} out of order");
        }
    }
}
