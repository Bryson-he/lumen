//! A small HTTPS GET, using the networking that comes with Windows (WinHTTP),
//! so Lumen carries no TLS library of its own. Blocking: call it from a
//! thread that may wait.

use windows::{
    Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
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
    let answer = request(&Request { url: &format!("https://{host}{path}"), method: "GET", headers: &[], body: None, timeout_ms, max_bytes })?;
    if answer.status != 200 {
        return Err(format!("{host} answered with an error ({})", answer.status));
    }
    Ok(answer.body)
}

/// A web request of any kind: the address, how to ask, and what to send.
pub struct Request<'a> {
    /// `https://…` or `http://…`, with an optional port.
    pub url: &'a str,
    /// GET, POST, PUT, PATCH or DELETE.
    pub method: &'a str,
    pub headers: &'a [(String, String)],
    pub body: Option<&'a [u8]>,
    pub timeout_ms: i32,
    /// The most the answer may hold.
    pub max_bytes: usize,
}

/// What came back: the status (200, 404, …) and the body, whatever the status.
#[derive(Debug)]
pub struct Response {
    pub status: u32,
    pub body: Vec<u8>,
}

/// An address taken apart: whether it is https, the host, the port and the path with its query.
pub fn split_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (secure, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (_, Some(rest)) => (false, rest),
        _ => return Err("a web address must start with https:// or http://".into()),
    };
    let (authority, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(authority, path)| (authority, format!("/{path}")));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().map_err(|_| format!("\"{port}\" is not a port number"))?),
        None => (authority, if secure { 443 } else { 80 }),
    };
    let host_ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
    if !host_ok || path.chars().any(|c| c.is_control() || c == ' ') {
        return Err(format!("\"{url}\" is not a web address"));
    }
    Ok((secure, host.to_owned(), port, path))
}

/// Sends a web request and returns the answer, whatever its status. Fails
/// only if the server could not be reached, took too long, or sent too much.
pub fn request(request: &Request<'_>) -> Result<Response, String> {
    let (secure, host, port, path) = split_url(request.url)?;
    let method = request.method.to_ascii_uppercase();
    if !["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str()) {
        return Err(format!("\"{}\" is not a kind of request; use GET, POST, PUT, PATCH or DELETE", request.method));
    }
    let mut headers = String::new();
    for (name, value) in request.headers {
        if name.is_empty() || name.chars().any(|c| c.is_control() || c == ':' || c == ' ') || value.chars().any(|c| c.is_control()) {
            return Err(format!("\"{}\" cannot be sent as a header", name.escape_debug()));
        }
        headers.push_str(&format!("{name}: {value}\r\n"));
    }
    let body = request.body.unwrap_or_default();
    let (timeout_ms, max_bytes, host) = (request.timeout_ms, request.max_bytes, host.as_str());
    unsafe {
        // A plain product name: nothing about the PC or the person goes in the request.
        let session = WinHttpOpen(w!("Lumen"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if session.is_null() {
            return Err("Windows networking is not available".into());
        }
        let session = Handle(session);
        let _ = WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);

        let connection = WinHttpConnect(session.0, &HSTRING::from(host), port, 0);
        if connection.is_null() {
            return Err(format!("could not reach {host}"));
        }
        let connection = Handle(connection);

        let flags = WINHTTP_OPEN_REQUEST_FLAGS(if secure { WINHTTP_FLAG_SECURE.0 } else { 0 });
        let handle = WinHttpOpenRequest(connection.0, &HSTRING::from(method.as_str()), &HSTRING::from(path.as_str()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags);
        if handle.is_null() {
            return Err(format!("could not ask {host}"));
        }
        let handle = Handle(handle);

        let header_text: Vec<u16> = headers.encode_utf16().collect();
        let sent = WinHttpSendRequest(handle.0, if header_text.is_empty() { None } else { Some(&header_text) }, if body.is_empty() { None } else { Some(body.as_ptr().cast()) }, body.len() as u32, body.len() as u32, 0);
        sent.map_err(|_| format!("could not reach {host} (no connection?)"))?;
        WinHttpReceiveResponse(handle.0, std::ptr::null_mut()).map_err(|_| format!("{host} did not answer"))?;

        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(handle.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some((&raw mut status).cast()), &mut size, std::ptr::null_mut()).map_err(|_| format!("{host} gave no answer"))?;

        let mut answer = Vec::new();
        loop {
            let mut available = 0u32;
            WinHttpQueryDataAvailable(handle.0, &mut available).map_err(|_| format!("the answer from {host} was cut short"))?;
            if available == 0 {
                break;
            }
            if answer.len() + available as usize > max_bytes {
                return Err(format!("the answer from {host} was too large"));
            }
            let start = answer.len();
            answer.resize(start + available as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(handle.0, answer[start..].as_mut_ptr().cast(), available, &mut read).map_err(|_| format!("the answer from {host} was cut short"))?;
            answer.truncate(start + read as usize);
            if read == 0 {
                break;
            }
        }
        Ok(Response { status, body: answer })
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

    #[test]
    fn an_address_is_taken_apart() {
        assert_eq!(split_url("https://example.com/a/b?c=1"), Ok((true, "example.com".into(), 443, "/a/b?c=1".into())));
        assert_eq!(split_url("http://homeassistant.local:8123/api/states/light.desk"), Ok((false, "homeassistant.local".into(), 8123, "/api/states/light.desk".into())));
        assert_eq!(split_url("http://192.168.1.20"), Ok((false, "192.168.1.20".into(), 80, "/".into())));
        for bad in ["example.com", "ftp://example.com", "https://", "https://exa mple.com/", "http://host:port/", "https://user@example.com/", "https://example.com/a b"] {
            assert!(split_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn odd_requests_are_refused_before_anything_is_sent() {
        let ask = |method: &str, headers: &[(String, String)]| request(&Request { url: "https://example.invalid/", method, headers, body: None, timeout_ms: 1000, max_bytes: 1000 }).unwrap_err();
        assert!(ask("TRACE", &[]).contains("not a kind of request"));
        assert!(ask("POST", &[("X-Bad\r\nInjected".into(), "1".into())]).contains("cannot be sent as a header"));
        assert!(ask("POST", &[("Authorization".into(), "Bearer x\r\nEvil: 1".into())]).contains("cannot be sent as a header"));
    }


    /// Needs the internet, so it only runs when asked for: cargo test -p lumen-win online -- --ignored
    #[test]
    #[ignore]
    fn online_a_secure_page_is_fetched() {
        let secure = get("example.com", "/", 10_000);
        assert!(secure.as_ref().is_ok_and(|page| page.contains("Example Domain")), "https: {secure:?}");
    }

}
