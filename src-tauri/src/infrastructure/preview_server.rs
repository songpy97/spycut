use std::{
    fmt::Write as _,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const MAX_RANGES: usize = 16;
const MAX_CONNECTIONS: usize = 16;

#[derive(Clone)]
pub struct PreviewServer {
    inner: Arc<PreviewServerInner>,
}

struct PreviewServerInner {
    address: SocketAddr,
    source: RwLock<Option<PublishedSource>>,
    active_connections: AtomicUsize,
}

#[derive(Clone)]
struct PublishedSource {
    token: String,
    path: PathBuf,
    length: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PreviewServerError {
    #[error("cannot bind the local preview server: {0}")]
    Bind(#[source] io::Error),
    #[error("cannot inspect the local preview server address: {0}")]
    LocalAddress(#[source] io::Error),
    #[error("cannot start the local preview server thread: {0}")]
    StartThread(#[source] io::Error),
    #[error("cannot canonicalize the preview source: {0}")]
    Canonicalize(#[source] io::Error),
    #[error("cannot inspect the preview source: {0}")]
    Metadata(#[source] io::Error),
    #[error("the preview source is not a regular file")]
    NotAFile,
    #[error("the preview server state is unavailable")]
    StateUnavailable,
}

impl PreviewServer {
    pub fn start() -> Result<Self, PreviewServerError> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(PreviewServerError::Bind)?;
        let address = listener
            .local_addr()
            .map_err(PreviewServerError::LocalAddress)?;
        let inner = Arc::new(PreviewServerInner {
            address,
            source: RwLock::new(None),
            active_connections: AtomicUsize::new(0),
        });
        let weak = Arc::downgrade(&inner);
        thread::Builder::new()
            .name("spycut-preview-listener".into())
            .spawn(move || accept_connections(listener, weak))
            .map_err(PreviewServerError::StartThread)?;
        Ok(Self { inner })
    }

    pub fn publish_source(&self, path: &Path) -> Result<String, PreviewServerError> {
        let canonical = path
            .canonicalize()
            .map_err(PreviewServerError::Canonicalize)?;
        let metadata = canonical.metadata().map_err(PreviewServerError::Metadata)?;
        if !metadata.is_file() {
            return Err(PreviewServerError::NotAFile);
        }
        let token = uuid::Uuid::new_v4().to_string();
        let source = PublishedSource {
            token: token.clone(),
            path: canonical,
            length: metadata.len(),
        };
        *self
            .inner
            .source
            .write()
            .map_err(|_| PreviewServerError::StateUnavailable)? = Some(source);
        Ok(format!(
            "http://localhost:{}/media/{token}",
            self.inner.address.port()
        ))
    }
}

impl Drop for PreviewServerInner {
    fn drop(&mut self) {
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
    }
}

fn accept_connections(listener: TcpListener, inner: Weak<PreviewServerInner>) {
    while let Ok((mut stream, _peer)) = listener.accept() {
        let Some(inner) = inner.upgrade() else {
            break;
        };
        let previous = inner.active_connections.fetch_add(1, Ordering::AcqRel);
        if previous >= MAX_CONNECTIONS {
            inner.active_connections.fetch_sub(1, Ordering::AcqRel);
            let _ = write_empty_response(&mut stream, "503 Service Unavailable", &[]);
            continue;
        }
        let connection_inner = inner.clone();
        if thread::Builder::new()
            .name("spycut-preview-connection".into())
            .spawn(move || {
                let _guard = ConnectionGuard(connection_inner.clone());
                let _ = handle_connection(&mut stream, &connection_inner);
            })
            .is_err()
        {
            inner.active_connections.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

struct ConnectionGuard(Arc<PreviewServerInner>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.active_connections.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    range: Option<String>,
}

fn handle_connection(stream: &mut TcpStream, inner: &PreviewServerInner) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let request = match read_request(stream) {
        Ok(Some(request)) => request,
        Ok(None) => return Ok(()),
        Err(_) => return write_empty_response(stream, "400 Bad Request", &[]),
    };

    if request.method == "OPTIONS" {
        return write_empty_response(
            stream,
            "204 No Content",
            &[
                ("Access-Control-Allow-Origin", "*".into()),
                ("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS".into()),
                ("Access-Control-Allow-Headers", "Range".into()),
                ("Access-Control-Allow-Private-Network", "true".into()),
            ],
        );
    }
    if request.method != "GET" && request.method != "HEAD" {
        return write_empty_response(
            stream,
            "405 Method Not Allowed",
            &[("Allow", "GET, HEAD, OPTIONS".into())],
        );
    }

    let source = inner
        .source
        .read()
        .ok()
        .and_then(|source| source.clone())
        .filter(|source| request.path == format!("/media/{}", source.token));
    let Some(source) = source else {
        return write_empty_response(stream, "404 Not Found", &[]);
    };
    serve_source(stream, &request, &source)
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut header = Vec::with_capacity(4 * 1024);
    let mut buffer = [0_u8; 4 * 1024];
    let header_end = loop {
        if let Some(position) = header.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if header.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request headers are too large",
            ));
        }
        let remaining = MAX_HEADER_BYTES - header.len();
        let requested = buffer.len().min(remaining);
        let count = stream.read(&mut buffer[..requested])?;
        if count == 0 {
            if header.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request headers ended early",
            ));
        }
        header.extend_from_slice(&buffer[..count]);
    };
    let text = std::str::from_utf8(&header[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "headers are not UTF-8"))?;
    let mut lines = text.split("\r\n");
    let mut request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "request line is missing"))?
        .split_whitespace();
    let method = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "method is missing"))?;
    let path = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path is missing"))?;
    let version = request_line
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "HTTP version is missing"))?;
    if request_line.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request line is invalid",
        ));
    }
    let mut range = None;
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "header line is invalid",
            ));
        };
        if name.eq_ignore_ascii_case("range") {
            range = Some(value.trim().to_string());
        }
    }
    Ok(Some(HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        range,
    }))
}

fn serve_source(
    stream: &mut TcpStream,
    request: &HttpRequest,
    source: &PublishedSource,
) -> io::Result<()> {
    let head_only = request.method == "HEAD";
    let Some(range_header) = request.range.as_deref() else {
        write_headers(stream, "200 OK", source.length, "video/mp4", &[])?;
        if !head_only && source.length > 0 {
            let mut file = File::open(&source.path)?;
            stream_file_range(stream, &mut file, 0, source.length)?;
        }
        return Ok(());
    };

    let ranges = match parse_range_header(range_header, source.length) {
        Ok(ranges) => ranges,
        Err(_) => {
            return write_empty_response(
                stream,
                "416 Range Not Satisfiable",
                &[("Content-Range", format!("bytes */{}", source.length))],
            );
        }
    };
    if ranges.len() == 1 {
        let range = ranges[0];
        write_headers(
            stream,
            "206 Partial Content",
            range.length(),
            "video/mp4",
            &[(
                "Content-Range",
                format!("bytes {}-{}/{}", range.start, range.end, source.length),
            )],
        )?;
        if !head_only {
            let mut file = File::open(&source.path)?;
            stream_file_range(stream, &mut file, range.start, range.length())?;
        }
        return Ok(());
    }

    serve_multiple_ranges(stream, source, &ranges, head_only)
}

fn serve_multiple_ranges(
    stream: &mut TcpStream,
    source: &PublishedSource,
    ranges: &[ByteRange],
    head_only: bool,
) -> io::Result<()> {
    let boundary = format!("spycut-{}", uuid::Uuid::new_v4().simple());
    let parts = ranges
        .iter()
        .map(|range| {
            format!(
                "--{boundary}\r\nContent-Type: video/mp4\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                range.start, range.end, source.length
            )
        })
        .collect::<Vec<_>>();
    let closing = format!("--{boundary}--\r\n");
    let content_length = parts
        .iter()
        .zip(ranges)
        .try_fold(0_u64, |total, (part, range)| {
            total
                .checked_add(part.len() as u64)?
                .checked_add(range.length())?
                .checked_add(2)
        })
        .and_then(|total| total.checked_add(closing.len() as u64))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "range response is too large"))?;
    write_headers(
        stream,
        "206 Partial Content",
        content_length,
        &format!("multipart/byteranges; boundary={boundary}"),
        &[],
    )?;
    if head_only {
        return Ok(());
    }
    let mut file = File::open(&source.path)?;
    for (part, range) in parts.iter().zip(ranges) {
        stream.write_all(part.as_bytes())?;
        stream_file_range(stream, &mut file, range.start, range.length())?;
        stream.write_all(b"\r\n")?;
    }
    stream.write_all(closing.as_bytes())
}

fn stream_file_range(
    stream: &mut TcpStream,
    file: &mut File,
    start: u64,
    length: u64,
) -> io::Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = length;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    while remaining > 0 {
        let requested = buffer.len().min(remaining as usize);
        let count = file.read(&mut buffer[..requested])?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "preview source ended inside a requested range",
            ));
        }
        stream.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    Ok(())
}

fn write_headers(
    stream: &mut TcpStream,
    status: &str,
    content_length: u64,
    content_type: &str,
    extra_headers: &[(&str, String)],
) -> io::Result<()> {
    let mut header = String::with_capacity(512);
    let _ = write!(
        header,
        "HTTP/1.1 {status}\r\nContent-Length: {content_length}\r\nContent-Type: {content_type}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Expose-Headers: Accept-Ranges, Content-Length, Content-Range\r\nAccess-Control-Allow-Private-Network: true\r\nCross-Origin-Resource-Policy: cross-origin\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n"
    );
    for (name, value) in extra_headers {
        let _ = write!(header, "{name}: {value}\r\n");
    }
    header.push_str("\r\n");
    stream.write_all(header.as_bytes())
}

fn write_empty_response(
    stream: &mut TcpStream,
    status: &str,
    extra_headers: &[(&str, String)],
) -> io::Result<()> {
    write_headers(
        stream,
        status,
        0,
        "text/plain; charset=utf-8",
        extra_headers,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn length(self) -> u64 {
        self.end - self.start + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangeError {
    Invalid,
    Unsatisfiable,
}

fn parse_range_header(value: &str, full_length: u64) -> Result<Vec<ByteRange>, RangeError> {
    let value = value.trim();
    let encoded = value.strip_prefix("bytes=").ok_or(RangeError::Invalid)?;
    if full_length == 0 || encoded.trim().is_empty() {
        return Err(RangeError::Unsatisfiable);
    }
    let mut ranges = Vec::new();
    for item in encoded.split(',') {
        if ranges.len() >= MAX_RANGES {
            return Err(RangeError::Invalid);
        }
        let item = item.trim();
        let (start, end) = item.split_once('-').ok_or(RangeError::Invalid)?;
        if start.is_empty() {
            let suffix = end.parse::<u64>().map_err(|_| RangeError::Invalid)?;
            if suffix == 0 {
                continue;
            }
            ranges.push(ByteRange {
                start: full_length.saturating_sub(suffix),
                end: full_length - 1,
            });
            continue;
        }
        let start = start.parse::<u64>().map_err(|_| RangeError::Invalid)?;
        if start >= full_length {
            continue;
        }
        let end = if end.is_empty() {
            full_length - 1
        } else {
            let requested_end = end.parse::<u64>().map_err(|_| RangeError::Invalid)?;
            if requested_end < start {
                return Err(RangeError::Invalid);
            }
            requested_end.min(full_length - 1)
        };
        ranges.push(ByteRange { start, end });
    }
    if ranges.is_empty() {
        Err(RangeError::Unsatisfiable)
    } else {
        Ok(ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Shutdown;

    #[test]
    fn parses_closed_open_suffix_and_multiple_ranges() {
        assert_eq!(
            parse_range_header("bytes=0-499", 1_000).unwrap(),
            vec![ByteRange { start: 0, end: 499 }]
        );
        assert_eq!(
            parse_range_header("bytes=500-", 1_000).unwrap(),
            vec![ByteRange {
                start: 500,
                end: 999
            }]
        );
        assert_eq!(
            parse_range_header("bytes=-200", 1_000).unwrap(),
            vec![ByteRange {
                start: 800,
                end: 999
            }]
        );
        assert_eq!(
            parse_range_header("bytes=0-0, 900-1200", 1_000).unwrap(),
            vec![
                ByteRange { start: 0, end: 0 },
                ByteRange {
                    start: 900,
                    end: 999
                }
            ]
        );
    }

    #[test]
    fn rejects_invalid_or_unsatisfiable_ranges() {
        assert_eq!(
            parse_range_header("items=0-1", 100),
            Err(RangeError::Invalid)
        );
        assert_eq!(
            parse_range_header("bytes=20-10", 100),
            Err(RangeError::Invalid)
        );
        assert_eq!(
            parse_range_header("bytes=100-", 100),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            parse_range_header("bytes=-0", 100),
            Err(RangeError::Unsatisfiable)
        );
    }

    #[test]
    fn streams_full_ranges_larger_than_one_megabyte_and_rotates_tokens() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("large preview.mp4");
        let first_bytes = (0..2_500_000)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        std::fs::write(&first_path, &first_bytes).unwrap();

        let server = PreviewServer::start().unwrap();
        let first_url = server.publish_source(&first_path).unwrap();
        let first_route = route_from_url(&first_url);
        let response = request(
            server.inner.address,
            &first_route,
            "GET",
            Some("bytes=100-1500100"),
        );
        let (headers, body) = split_response(&response);
        assert!(headers.starts_with("HTTP/1.1 206 Partial Content"));
        assert!(headers.contains("Content-Length: 1500001\r\n"));
        assert!(headers.contains("Content-Range: bytes 100-1500100/2500000\r\n"));
        assert_eq!(body, &first_bytes[100..=1_500_100]);

        let head = request(server.inner.address, &first_route, "HEAD", None);
        let (headers, body) = split_response(&head);
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(headers.contains("Content-Length: 2500000\r\n"));
        assert!(body.is_empty());

        let second_path = directory.path().join("second.mp4");
        std::fs::write(&second_path, b"second source").unwrap();
        let second_url = server.publish_source(&second_path).unwrap();
        let stale = request(server.inner.address, &first_route, "GET", None);
        assert!(stale.starts_with(b"HTTP/1.1 404 Not Found\r\n"));
        let current = request(
            server.inner.address,
            &route_from_url(&second_url),
            "GET",
            None,
        );
        let (_, body) = split_response(&current);
        assert_eq!(body, b"second source");
    }

    #[test]
    fn serves_multiple_ranges_and_reports_unsatisfiable_requests() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("multi.mp4");
        std::fs::write(&source, b"0123456789abcdef").unwrap();
        let server = PreviewServer::start().unwrap();
        let route = route_from_url(&server.publish_source(&source).unwrap());

        let response = request(server.inner.address, &route, "GET", Some("bytes=0-1,14-15"));
        let (headers, body) = split_response(&response);
        assert!(headers.starts_with("HTTP/1.1 206 Partial Content"));
        assert!(headers.contains("Content-Type: multipart/byteranges; boundary="));
        assert!(body.windows(2).any(|window| window == b"01"));
        assert!(body.windows(2).any(|window| window == b"ef"));

        let invalid = request(server.inner.address, &route, "GET", Some("bytes=99-"));
        let (headers, body) = split_response(&invalid);
        assert!(headers.starts_with("HTTP/1.1 416 Range Not Satisfiable"));
        assert!(headers.contains("Content-Range: bytes */16\r\n"));
        assert!(body.is_empty());
    }

    #[test]
    fn handles_preflight_and_rejects_unsupported_methods() {
        let server = PreviewServer::start().unwrap();

        let preflight = request(server.inner.address, "/media/unknown", "OPTIONS", None);
        let (headers, body) = split_response(&preflight);
        assert!(headers.starts_with("HTTP/1.1 204 No Content"));
        assert!(headers.contains("Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n"));
        assert!(body.is_empty());

        let unsupported = request(server.inner.address, "/media/unknown", "POST", None);
        let (headers, body) = split_response(&unsupported);
        assert!(headers.starts_with("HTTP/1.1 405 Method Not Allowed"));
        assert!(headers.contains("Allow: GET, HEAD, OPTIONS\r\n"));
        assert!(body.is_empty());
    }

    fn route_from_url(url: &str) -> String {
        let without_scheme = url.strip_prefix("http://").unwrap();
        let slash = without_scheme.find('/').unwrap();
        without_scheme[slash..].to_string()
    }

    fn request(address: SocketAddr, path: &str, method: &str, range: Option<&str>) -> Vec<u8> {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let range = range
            .map(|value| format!("Range: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{range}Connection: close\r\n\r\n"
        )
        .unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    }

    fn split_response(response: &[u8]) -> (&str, &[u8]) {
        let position = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap();
        (
            std::str::from_utf8(&response[..position + 4]).unwrap(),
            &response[position + 4..],
        )
    }
}
