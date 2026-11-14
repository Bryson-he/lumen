//! A small HTTPS GET, using the networking that comes with Windows (WinHTTP),
//! so Lumen carries no TLS library of its own. Blocking: call it from a
//! thread that may wait.

use windows::{
    Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
        WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    },
    core::{HSTRING, PCWSTR, w},
};

/// The most a response may hold; anything longer is refused.
const MAX_BYTES: usize = 1024 * 1024;

/// Closes a WinHTTP handle when it goes out of scope.
struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// Fetches `https://{host}{path}` and returns the body as text. `path`
/// starts with `/` and carries the query. Fails on anything but a 200 answer,
/// after about `timeout_ms`, or if the body is too large or not text.
pub fn get(host: &str, path: &str, timeout_ms: i32) -> Result<String, String> {
    String::from_utf8(get_bytes(host, path, timeout_ms, MAX_BYTES)?).map_err(|_| format!("the answer from {host} was not text"))
}

/// As [`get`], for a body that need not be text and may be up to `max_bytes` long (a file to download).
pub fn get_bytes(host: &str, path: &str, timeout_ms: i32, max_bytes: usize) -> Result<Vec<u8>, String> {
    unsafe {
        // A plain product name: nothing about the PC or the person goes in the request.
        let session = WinHttpOpen(w!("Lumen"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() {
            return Err("Windows networking is not available".into());
        }
        let session = Handle(session);
        let _ = WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);

        let connection = WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connection.is_null() {
            return Err(format!("could not reach {host}"));
        }
        let connection = Handle(connection);

        let request = WinHttpOpenRequest(connection.0, w!("GET"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_OPEN_REQUEST_FLAGS(WINHTTP_FLAG_SECURE.0));
        if request.is_null() {
            return Err(format!("could not ask {host}"));
        }
        let request = Handle(request);

        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|_| format!("could not reach {host} (no internet?)"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|_| format!("{host} did not answer"))?;

        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(request.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&raw mut status).cast()), &mut size, std::ptr::null_mut()).map_err(|_| format!("{host} gave no answer"))?;
        if status != 200 {
            return Err(format!("{host} answered with an error ({status})"));
        }

        let mut body = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(request.0, &mut available).map_err(|_| format!("the answer from {host} was cut short"))?;
            if available == 0 {
                break;
            }
            if body.len() + available as usize > max_bytes {
                return Err(format!("the answer from {host} was too large"));
            }
            let start = body.len();
            body.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(request.0, body[start..].as_mut_ptr().cast(), available, &mut read).map_err(|_| format!("the answer from {host} was cut short"))?;
            body.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok(body)
    }
}

/// Makes text safe to put in a query: everything but letters, digits and a few marks becomes `%XX`.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_text_is_encoded() {
        assert_eq!(encode("São Paulo, BR"), "S%C3%A3o%20Paulo%2C%20BR");
        assert_eq!(encode("plain-Text_1.0~"), "plain-Text_1.0~");
        assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn a_host_that_does_not_exist_is_an_error_not_a_hang() {
        let started = std::time::Instant::now();
        assert!(get("no-such-host.invalid", "/", 3000).is_err());
        assert!(started.elapsed().as_secs() < 20);
    }
}
