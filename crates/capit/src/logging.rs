// Author: Dustin Pilgrim
// License: MIT

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use eventline::runtime::{self, LogLevel};

use crate::paths::ensure_parent_dir;

pub fn init_logging(log_path: &Path, verbose: bool) -> Result<(), String> {
    ensure_parent_dir(log_path).map_err(|e| format!("create log dir: {e}"))?;

    block_on(runtime::init());

    runtime::enable_file_output(log_path).map_err(|e| format!("enable file output: {e}"))?;

    if verbose {
        runtime::enable_console_output(true);
        runtime::enable_console_color(true);
        runtime::set_log_level(LogLevel::Debug);
        eprintln!("eventline: console logging enabled (debug level)");
    } else {
        runtime::set_log_level(LogLevel::Info);
    }

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
