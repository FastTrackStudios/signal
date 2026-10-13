use block2::RcBlock;
use objc2::rc::Weak;
use objc2_core_foundation::{
    kCFAllocatorDefault, kCFRunLoopCommonModes, CFRetained, CFRunLoop, CFRunLoopTimer,
    CFTimeInterval,
};

pub struct TimerHandle {
    run_loop: Weak<CFRunLoop>,
    timer: CFRetained<CFRunLoopTimer>,
}

impl TimerHandle {
    pub fn new(interval: CFTimeInterval, closure: impl Fn() + 'static) -> Option<Self> {
        let run_loop = CFRunLoop::current()?;

        let block = RcBlock::new(move |_| closure());

        let allocator = unsafe { kCFAllocatorDefault };
        let timer =
            unsafe { CFRunLoopTimer::with_handler(allocator, 0.0, interval, 0, 0, Some(&block)) }?;

        // FTS PATCH — common modes, NOT default mode.
        //
        // A timer registered only in kCFRunLoopDefaultMode stops firing
        // whenever AppKit runs the run loop modally, which it does for the
        // entire duration of a live window resize (NSEventTrackingRunLoopMode)
        // and for menu tracking. This timer drives `on_frame`, so a
        // default-mode registration freezes the window handler for the whole
        // drag: no relayout, no redraw, and AppKit stretches the stale surface
        // until the mouse comes up. kCFRunLoopCommonModes covers default AND
        // tracking. Still kCFRunLoopDefaultMode upstream at 0.3.3.
        let loop_mode = unsafe { kCFRunLoopCommonModes };
        run_loop.add_timer(Some(&timer), loop_mode);

        Some(Self { run_loop: Weak::from_retained(&run_loop.into()), timer })
    }
}

impl Drop for TimerHandle {
    fn drop(&mut self) {
        let Some(run_loop) = self.run_loop.load() else {
            return;
        };

        // Must match the mode used in `new` above, or the timer leaks.
        let loop_mode = unsafe { kCFRunLoopCommonModes };

        run_loop.remove_timer(Some(&self.timer), loop_mode);
    }
}
