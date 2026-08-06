use std::collections::HashMap;
use std::io::{self, Read, Write};

use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, Name, prelude::*,
};

/// A request sent from a `po` client to the context daemon.
pub enum Request {
    /// Run a piece of code and wait for it to finish. `runtime` is the
    /// interpreter to invoke (e.g. `"sh"`, `"python3"`); `None` means the
    /// code came from a shebang and should be executed directly instead.
    Run {
        code: String,
        runtime: Option<String>,
    },
    /// Ask the daemon to shut down.
    Kill,
    /// Liveness check.
    Ping,
}

/// A response sent from the daemon back to a `po` client.
pub enum Response {
    Ran {
        stdout: String,
        stderr: String,
        exit_code: i32,
        elapsed_ms: u64,
        new_env: HashMap<String, String>,
    },
    Killed,
    Pong,
    Error {
        message: String,
    },
}

// ---- wire format --------------------------------------------------------
//
// Hand-rolled binary encoding (no serde): every message is a single tag
// byte followed by its fields, each string/blob prefixed with a
// little-endian u32 byte length. This is transported over the socket by
// [`write_frame`]/[`read_frame`], which just wrap the whole message in one
// more length prefix.

fn put_u32(buf: &mut Vec<u8>, n: u32) {
    buf.extend_from_slice(&n.to_le_bytes());
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn put_map(buf: &mut Vec<u8>, map: &HashMap<String, String>) {
    put_u32(buf, map.len() as u32);
    for (k, v) in map {
        put_str(buf, k);
        put_str(buf, v);
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn err(msg: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
    }

    fn u8(&mut self) -> io::Result<u8> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or_else(|| Self::err("unexpected end of message"))?;
        self.pos += 1;
        Ok(b)
    }

    fn u32(&mut self) -> io::Result<u32> {
        let end = self.pos + 4;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| Self::err("unexpected end of message"))?;
        self.pos = end;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn u64(&mut self) -> io::Result<u64> {
        let end = self.pos + 8;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| Self::err("unexpected end of message"))?;
        self.pos = end;
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn str(&mut self) -> io::Result<String> {
        let len = self.u32()? as usize;
        let end = self.pos + len;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| Self::err("unexpected end of message"))?;
        self.pos = end;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn map(&mut self) -> io::Result<HashMap<String, String>> {
        let count = self.u32()?;
        let mut result = HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let k = self.str()?;
            let v = self.str()?;
            result.insert(k, v);
        }
        Ok(result)
    }
}

pub fn encode_request(req: &Request) -> Vec<u8> {
    let mut buf = Vec::new();
    match req {
        Request::Run { code, runtime } => {
            buf.push(0);
            match runtime {
                Some(rt) => {
                    buf.push(1);
                    put_str(&mut buf, rt);
                }
                None => buf.push(0),
            }
            put_str(&mut buf, code);
        }
        Request::Kill => buf.push(1),
        Request::Ping => buf.push(2),
    }
    buf
}

pub fn decode_request(data: &[u8]) -> io::Result<Request> {
    let mut r = Reader::new(data);
    match r.u8()? {
        0 => {
            let has_runtime = r.u8()?;
            let runtime = if has_runtime == 1 {
                Some(r.str()?)
            } else {
                None
            };
            let code = r.str()?;
            Ok(Request::Run { code, runtime })
        }
        1 => Ok(Request::Kill),
        2 => Ok(Request::Ping),
        tag => Err(Reader::err(&format!("unknown request tag: {tag}"))),
    }
}

pub fn encode_response(resp: &Response) -> Vec<u8> {
    let mut buf = Vec::new();
    match resp {
        Response::Ran {
            stdout,
            stderr,
            exit_code,
            elapsed_ms,
            new_env,
        } => {
            buf.push(0);
            put_str(&mut buf, stdout);
            put_str(&mut buf, stderr);
            put_u32(&mut buf, *exit_code as u32);
            buf.extend_from_slice(&elapsed_ms.to_le_bytes());
            put_map(&mut buf, new_env);
        }
        Response::Killed => buf.push(1),
        Response::Pong => buf.push(2),
        Response::Error { message } => {
            buf.push(3);
            put_str(&mut buf, message);
        }
    }
    buf
}

pub fn decode_response(data: &[u8]) -> io::Result<Response> {
    let mut r = Reader::new(data);
    match r.u8()? {
        0 => {
            let stdout = r.str()?;
            let stderr = r.str()?;
            let exit_code = r.u32()? as i32;
            let elapsed_ms = r.u64()?;
            let new_env = r.map()?;
            Ok(Response::Ran {
                stdout,
                stderr,
                exit_code,
                elapsed_ms,
                new_env,
            })
        }
        1 => Ok(Response::Killed),
        2 => Ok(Response::Pong),
        3 => Ok(Response::Error { message: r.str()? }),
        tag => Err(Reader::err(&format!("unknown response tag: {tag}"))),
    }
}

/// Build the socket name for a given context. `ctx` should already be
/// normalized (lowercased); an empty string maps to the "default" context.
/// Prefers a namespaced name (Linux abstract socket / Windows named pipe);
/// falls back to a filesystem path (e.g. macOS, which has no namespace).
pub fn socket_name(ctx: &str) -> io::Result<Name<'static>> {
    let ctx = if ctx.is_empty() { "default" } else { ctx };
    let raw = format!("edc-po-{ctx}.sock");
    if GenericNamespaced::is_supported() {
        raw.to_ns_name::<GenericNamespaced>()
    } else {
        format!("/tmp/{raw}").to_fs_name::<GenericFilePath>()
    }
}

/// Write a length-prefixed frame: a little-endian `u32` byte count followed
/// by the raw message bytes.
pub fn write_frame(stream: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let len = u32::try_from(bytes.len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    stream.write_all(&len.to_le_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

/// Read one length-prefixed frame written by [`write_frame`].
pub fn read_frame(stream: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip_run_with_interpreter() {
        let req = Request::Run {
            code: "echo hi".to_string(),
            runtime: Some("sh".to_string()),
        };
        let decoded = decode_request(&encode_request(&req)).unwrap();
        match decoded {
            Request::Run { code, runtime } => {
                assert_eq!(code, "echo hi");
                assert_eq!(runtime.as_deref(), Some("sh"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_roundtrip_run_via_shebang() {
        let req = Request::Run {
            code: "#!/bin/sh\necho hi\n".to_string(),
            runtime: None,
        };
        let decoded = decode_request(&encode_request(&req)).unwrap();
        match decoded {
            Request::Run { runtime, .. } => assert_eq!(runtime, None),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_roundtrip_kill_ping() {
        assert!(matches!(
            decode_request(&encode_request(&Request::Kill)).unwrap(),
            Request::Kill
        ));
        assert!(matches!(
            decode_request(&encode_request(&Request::Ping)).unwrap(),
            Request::Ping
        ));
    }

    #[test]
    fn response_roundtrip_ran() {
        let mut new_env = HashMap::new();
        new_env.insert("FOO".to_string(), "bar".to_string());
        let resp = Response::Ran {
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            exit_code: -1,
            elapsed_ms: 12345,
            new_env,
        };
        let decoded = decode_response(&encode_response(&resp)).unwrap();
        match decoded {
            Response::Ran {
                stdout,
                stderr,
                exit_code,
                elapsed_ms,
                new_env,
            } => {
                assert_eq!(stdout, "out");
                assert_eq!(stderr, "err");
                assert_eq!(exit_code, -1);
                assert_eq!(elapsed_ms, 12345);
                assert_eq!(new_env.get("FOO"), Some(&"bar".to_string()));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_error() {
        let resp = Response::Error {
            message: "boom".to_string(),
        };
        let decoded = decode_response(&encode_response(&resp)).unwrap();
        match decoded {
            Response::Error { message } => assert_eq!(message, "boom"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn frame_roundtrip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"hello").unwrap();
        let mut cursor = io::Cursor::new(buf);
        let decoded = read_frame(&mut cursor).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn socket_name_default_context() {
        assert!(socket_name("").is_ok());
    }

    #[test]
    fn socket_name_custom_context() {
        assert!(socket_name("proj").is_ok());
    }
}
