// Author: Dustin Pilgrim
// License: MIT

use capit_ipc::Response;

pub fn print_response(resp: Response) {
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

        Response::UiConfig { cfg } => {
            println!("theme: {:?}", cfg.theme);
            println!("accent_colour: 0x{:08X}", cfg.accent_colour);
            println!("bar_background_colour: 0x{:08X}", cfg.bar_background_colour);
        }
    }
}

pub fn print_outputs_or_fallback(resp: Response) {
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
