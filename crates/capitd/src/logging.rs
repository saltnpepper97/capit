// Author: Dustin Pilgrim
// License: MIT

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use eventline::runtime::{self, LogLevel};

pub fn init_logging(log_path: &Path, verbose: bool) -> Result<(), String> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create log dir: {e}"))?;
    }

    block_on(runtime::init());

    runtime::enable_file_output(log_path).map_err(|e| format!("enable file output: {e}"))?;

    // Explicitly control console output.
    runtime::enable_console_output(verbose);
    runtime::enable_console_color(verbose);

    if verbose {
        runtime::set_log_level(LogLevel::Debug);
    } else {
        runtime::set_log_level(LogLevel::Info);
    }

    // Optional: log after init so it respects the console setting.
    // (When verbose=false, this goes to file only.)
    eventline::debug!(
        "logging initialized: console={} level={:?} file={}",
        verbose,
        if verbose { LogLevel::Debug } else { LogLevel::Info },
        log_path.display()
    );

    Ok(())
}

fn block_on<F: Future>(mut fut: F) -> F::Output {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);

    let mut fut = unsafe { Pin::new_unchecked(&mut fut) };

    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
