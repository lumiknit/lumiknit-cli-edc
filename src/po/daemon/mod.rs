pub mod exec;

use std::collections::HashMap;
use std::io;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use interprocess::local_socket::{
    ListenerOptions, Stream as LocalStream, prelude::*,
};

use crate::po::ipc::{self, Request, Response};

/// Context state a daemon keeps in memory for its whole lifetime.
struct DaemonState {
    env: Mutex<HashMap<String, String>>,
}

/// Connect to the context daemon, starting one (detached, in the
/// background) if it isn't already running. Returns once a connection has
/// been established, or an error if the daemon never came up.
pub fn ensure_daemon(ctx: &str) -> io::Result<LocalStream> {
    if let Ok(stream) = connect(ctx) {
        return Ok(stream);
    }

    spawn_detached(ctx)?;

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match connect(ctx) {
            Ok(stream) => return Ok(stream),
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e),
        }
    }
}

fn connect(ctx: &str) -> io::Result<LocalStream> {
    let name = ipc::socket_name(ctx)?;
    LocalStream::connect(name)
}

#[cfg(unix)]
fn spawn_detached(ctx: &str) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("-start")
        .arg("-x")
        .arg(ctx)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(windows)]
fn spawn_detached(ctx: &str) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let exe = std::env::current_exe()?;
    Command::new(exe)
        .arg("-start")
        .arg("-x")
        .arg(ctx)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()?;
    Ok(())
}

/// Send a request to the daemon for `ctx` (starting it if needed) and
/// return its response.
pub fn send_request(ctx: &str, req: &Request) -> io::Result<Response> {
    let mut stream = ensure_daemon(ctx)?;
    ipc::write_frame(&mut stream, &ipc::encode_request(req))?;
    let bytes = ipc::read_frame(&mut stream)?;
    ipc::decode_response(&bytes)
}

/// Ask the daemon for `ctx` to shut down. Returns `Ok(true)` if a daemon
/// was reached and killed, `Ok(false)` if none was running.
pub fn kill(ctx: &str) -> io::Result<bool> {
    let Ok(mut stream) = connect(ctx) else {
        return Ok(false);
    };
    ipc::write_frame(&mut stream, &ipc::encode_request(&Request::Kill))?;
    let bytes = ipc::read_frame(&mut stream)?;
    let _: Response = ipc::decode_response(&bytes)?;
    Ok(true)
}

/// Run the daemon loop for `ctx` in the current process. Blocks until a
/// `Kill` request is received.
pub fn run(ctx: &str) -> io::Result<()> {
    let name = ipc::socket_name(ctx)?;
    let listener = ListenerOptions::new().name(name).create_sync()?;
    let state = DaemonState {
        env: Mutex::new(HashMap::new()),
    };

    for conn in listener.incoming() {
        let mut conn = match conn {
            Ok(c) => c,
            Err(_) => continue,
        };
        let req = match ipc::read_frame(&mut conn)
            .and_then(|bytes| ipc::decode_request(&bytes))
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        match req {
            Request::Ping => {
                let _ = ipc::write_frame(
                    &mut conn,
                    &ipc::encode_response(&Response::Pong),
                );
            }
            Request::Kill => {
                let _ = ipc::write_frame(
                    &mut conn,
                    &ipc::encode_response(&Response::Killed),
                );
                break;
            }
            Request::Run { code, runtime } => {
                // Locking the whole run (script execution + SETENV merge)
                // is what makes execution a single-item queue: only one
                // script runs at a time, and the next one always sees the
                // previous one's SETENV updates.
                let mut env = state.env.lock().unwrap();
                let response =
                    match exec::run_shell(&code, runtime.as_deref(), &env) {
                        Ok(result) => {
                            env.extend(result.setenv.clone());
                            Response::Ran {
                                stdout: result.stdout,
                                stderr: result.stderr,
                                exit_code: result.exit_code,
                                elapsed_ms: result.elapsed_ms,
                                new_env: result.setenv,
                            }
                        }
                        Err(e) => Response::Error {
                            message: e.to_string(),
                        },
                    };
                drop(env);
                let _ = ipc::write_frame(
                    &mut conn,
                    &ipc::encode_response(&response),
                );
            }
        }
    }

    Ok(())
}
