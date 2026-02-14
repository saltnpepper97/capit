// Author: Dustin Pilgrim
// License: MIT

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use clap::{Parser, Subcommand};

use capit_core::{Mode, Target};
use capit_ipc::{Event, IpcClient, Request, Response};

// eventline
use eventline::{debug, error, info};
use eventline::runtime::{self, LogLevel};

#[derive(Debug, Parser)]
#[command(name = "capit", version, about = "Capit — capture it.")]
struct Args {
    /// Override IPC socket path (default: $XDG_RUNTIME_DIR/capit.sock)
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Log to stderr (in addition to the log file)
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Override log file path (default: $XDG_STATE_HOME/capit/capit.log)
    #[arg(long)]
    log_file: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Show daemon status
    Status,

    /// List outputs (monitors)
    Outputs,

    /// Cancel active capture job
    Cancel,

    /// Show floating bar UI (lets you pick mode/target/options)
    Bar {
        /// Preselect a mode (screen/region/window/record)
        #[arg(long)]
        mode: Option<Mode>,
        /// Preselect an output name (e.g. DP-1)
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Start a region capture (mouse-driven overlay)
    Region {
        /// Optionally target a specific output by name
        #[arg(long, short = 'o')]
        output: Option<String>,
        /// Open the Bar UI instead of running overlay
        #[arg(long)]
        ui: bool,
    },

    /// Start a full-screen capture (headless by default)
    Screen {
        /// Capture a specific output by name, otherwise all screens
        #[arg(long, short = 'o')]
        output: Option<String>,
        /// Open the Bar UI instead of running headless
        #[arg(long)]
        ui: bool,
    },

    /// Start a window capture (headless by default)
    Window {
        /// Open the Bar UI instead of running headless
        #[arg(long)]
        ui: bool,
    },
}

fn main() {
    let args = Args::parse();

    // init logging first
    let log_path = args
        .log_file
        .clone()
        .unwrap_or_else(|| default_log_path("capit.log"));

    if let Err(e) = init_logging(&log_path, args.verbose) {
        // logging should never block normal usage
        eprintln!("capit: failed to init logging: {e}");
    }

    if let Err(e) = run(args) {
        // user-facing error
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    info!("starting client");
    debug!("parsed args: {:?}", args.cmd);

    let socket = args.socket.unwrap_or_else(default_socket_path);
    debug!("connecting to socket: {}", socket.display());

    let mut client = IpcClient::connect(&socket).map_err(|e| {
        error!("failed to connect to daemon: {e}");
        format!(
            "Failed to connect to daemon at {}: {e}\nIs capitd running?",
            socket.display()
        )
    })?;

    info!("connected to daemon");

    match args.cmd {
        Cmd::Status => {
            debug!("sending Status request");
            let resp = client.call(Request::Status).map_err(|e| format!("{e}"))?;
            print_response(resp);
        }

        Cmd::Outputs => {
            debug!("sending ListOutputs request");
            let resp = client.call(Request::ListOutputs).map_err(|e| format!("{e}"))?;
            match resp {
                Response::Outputs { outputs } => {
                    if outputs.is_empty() {
                        println!("(no outputs reported yet)");
                    } else {
                        for (i, o) in outputs.iter().enumerate() {
                            let name = o.name.as_deref().unwrap_or("(unnamed)");
                            println!(
                                "#{i}: {name} @ ({}, {}) {}x{} scale {}",
                                o.x, o.y, o.width, o.height, o.scale
                            );
                        }
                    }
                }
                other => print_response(other),
            }
        }

        Cmd::Cancel => {
            debug!("sending Cancel request");
            let resp = client.call(Request::Cancel).map_err(|e| format!("{e}"))?;
            print_response(resp);
        }

        Cmd::Bar { mode, output } => {
            debug!("bar command (mode={:?}, output={:?})", mode, output);
            let mode = mode.unwrap_or(Mode::Screen);
            let target = match output {
                Some(name) => Some(Target::OutputName(name)),
                None => Some(Target::AllScreens),
            };
            run_ui_session(&mut client, mode, target)?;
        }

        Cmd::Region { output, ui } => {
            debug!("region command (output={:?}, ui={})", output, ui);
            let target = output.map(Target::OutputName);

            if ui {
                return Err("bar UI not implemented yet; region overlay runs by default".into());
            } else {
                run_ui_session(&mut client, Mode::Region, target)?;
            }
        }

        Cmd::Screen { output, ui } => {
            debug!("screen command (output={:?}, ui={})", output, ui);
            let target = match output {
                Some(name) => Some(Target::OutputName(name)),
                None => Some(Target::AllScreens),
            };
            if ui {
                run_ui_session(&mut client, Mode::Screen, target)?;
            } else {
                start_capture(&mut client, Mode::Screen, target, false)?;
            }
        }

        Cmd::Window { ui } => {
            debug!("window command (ui={})", ui);
            if ui {
                run_ui_session(&mut client, Mode::Window, None)?;
            } else {
                start_capture(&mut client, Mode::Window, None, false)?;
            }
        }
    }

    info!("client done");
    Ok(())
}

fn start_capture(
    client: &mut IpcClient,
    mode: Mode,
    target: Option<Target>,
    with_ui: bool,
) -> Result<(), String> {
    debug!(
        "start_capture: mode={:?}, target={:?}, with_ui={}",
        mode, target, with_ui
    );

    let resp = client
        .call(Request::StartCapture {
            mode,
            target,
            with_ui,
        })
        .map_err(|e| format!("{e}"))?;

    print_response(resp);
    Ok(())
}

/// UI session: sends StartCapture with with_ui=true and waits for events
fn run_ui_session(
    client: &mut IpcClient,
    mode: Mode,
    target: Option<Target>,
) -> Result<(), String> {
    debug!(
        "run_ui_session: StartCapture mode={:?} target={:?}",
        mode, target
    );

    let resp = client
        .call(Request::StartCapture {
            mode,
            target,
            with_ui: true,
        })
        .map_err(|e| format!("{e}"))?;

    match resp {
        Response::Ok => {}
        other => {
            print_response(other);
            return Ok(());
        }
    }

    loop {
        let ev = client.next_event().map_err(|e| format!("{e}"))?;
        debug!("event: {:?}", ev);

        match ev {
            Event::CaptureFinished { path } => {
                // user-facing output
                println!("{path}");
                return Ok(());
            }
            Event::CaptureFailed { message } => {
                if message == "cancelled" {
                    return Ok(());
                }
                return Err(message);
            }
            _ => {}
        }
    }
}

fn print_response(resp: Response) {
    match resp {
        Response::Ok => println!("ok"),
        Response::Status { running, active_job } => {
            println!("running: {running}");
            match active_job {
                Some(m) => println!("active_job: {m:?}"),
                None => println!("active_job: none"),
            }
        }
        Response::Error { message } => eprintln!("error: {message}"),
        Response::Outputs { outputs } => println!("outputs: {}", outputs.len()),
    }
}

fn default_socket_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("capit.sock")
}

fn default_log_path(file: &str) -> PathBuf {
    // Prefer state dir for logs
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("capit").join(file)
}

fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn init_logging(log_path: &Path, verbose: bool) -> Result<(), String> {
    ensure_parent_dir(log_path).map_err(|e| format!("create log dir: {e}"))?;

    // eventline runtime init is async; run it with a tiny local block_on
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
