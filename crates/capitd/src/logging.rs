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

    // eventline runtime init is async; we run it with a tiny local block_on
    block_on(runtime::init());

    // Always log to file.
    runtime::enable_file_output(log_path).map_err(|e| format!("enable file output: {e}"))?;

    // Console output only when verbose.
    runtime::enable_console_output(verbose);
    runtime::enable_console_color(verbose);

    // Log level: Debug when verbose, Info otherwise.
    runtime::set_log_level(if verbose { LogLevel::Debug } else { LogLevel::Info });

    Ok(())
}

// -------------------- tiny async runner (no new deps) --------------------

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

    // SAFETY: we don't move `fut` after pinning.
    let mut fut = unsafe { Pin::new_unchecked(&mut fut) };

    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
