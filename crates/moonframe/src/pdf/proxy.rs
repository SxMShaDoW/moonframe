//! The way out for a web page made into a PDF: a proxy on this machine that
//! WebKit sends every connection of the page's session through. It looks a
//! host up itself, refuses it when any of its addresses is off the public
//! internet, and connects to an address it checked. So neither a redirect,
//! a frame or picture by name, nor a name whose answer changes between a
//! check and the connection (DNS rebinding) reaches this machine or its
//! network: the content rules only see an address's text.

use {
    std::{
        io::{self, Read, Write},
        net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
        sync::{
            Arc, OnceLock,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    },
    url::Url,
};

/// Longest request head read.
const MAX_HEAD: usize = 16 * 1024;
/// Most connections open at once; more are closed as they come.
const MAX_OPEN: usize = 64;
/// How long a request's head, a lookup's connection, or a quiet tunnel waits.
const WAIT: Duration = Duration::from_secs(30);

/// Which addresses a connection may reach.
type Allow = fn(IpAddr) -> bool;

/// The port of the proxy pages' sessions go through, started the first
/// time one asks; it reaches only the public internet.
pub(super) fn port() -> Result<u16, String> {
    static PORT: OnceLock<Result<u16, String>> = OnceLock::new();
    PORT.get_or_init(|| start(moonframe_core::public_address))
        .clone()
}

/// Listens on a free port of this machine, serving each connection on a
/// thread of its own.
fn start(allow: Allow) -> Result<u16, String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("cannot start the PDF proxy: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("cannot start the PDF proxy: {error}"))?
        .port();
    let open = Arc::new(AtomicUsize::new(0));
    thread::Builder::new()
        .name("pdf-proxy".to_owned())
        .spawn(move || {
            for client in listener.incoming().flatten() {
                if open.fetch_add(1, Ordering::SeqCst) >= MAX_OPEN {
                    open.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let held = Arc::clone(&open);
                let spawned = thread::Builder::new()
                    .name("pdf-proxy-connection".to_owned())
                    .spawn(move || {
                        if let Err(error) = serve(client, allow) {
                            tracing::debug!(%error, "PDF proxy connection ended");
                        }
                        held.fetch_sub(1, Ordering::SeqCst);
                    });
                if spawned.is_err() {
                    open.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })
        .map_err(|error| format!("cannot start the PDF proxy: {error}"))?;
    Ok(port)
}

/// One connection: a `CONNECT host:port` tunnel (what WebKit asks for), or
/// a plain `http://` request in absolute form, sent on with `Connection:
/// close`.
fn serve(mut client: TcpStream, allow: Allow) -> io::Result<()> {
    client.set_read_timeout(Some(WAIT))?;
    let (head, early) = read_head(&mut client)?;
    let head = String::from_utf8(head)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "a head not in UTF-8"))?;
    let mut lines = head.split("\r\n");
    let mut request = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(version), None) = (
        request.next(),
        request.next(),
        request.next(),
        request.next(),
    ) else {
        return refuse(&mut client, "400 Bad Request");
    };
    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = authority(target) else {
            return refuse(&mut client, "400 Bad Request");
        };
        let upstream = match connect(&host, port, allow) {
            Ok(upstream) => upstream,
            Err(status) => return refuse(&mut client, status),
        };
        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
        return splice(client, upstream, &early);
    }
    let Some((host, port, path)) = Url::parse(target)
        .ok()
        .filter(|url| url.scheme() == "http")
        .and_then(|url| {
            let host = match url.host()? {
                url::Host::Domain(name) => name.to_owned(),
                url::Host::Ipv4(ip) => ip.to_string(),
                url::Host::Ipv6(ip) => ip.to_string(),
            };
            let path = url[url::Position::BeforePath..url::Position::AfterQuery].to_owned();
            Some((host, url.port_or_known_default()?, path))
        })
    else {
        return refuse(&mut client, "400 Bad Request");
    };
    let mut upstream = match connect(&host, port, allow) {
        Ok(upstream) => upstream,
        Err(status) => return refuse(&mut client, status),
    };
    let mut sent = format!("{method} {path} {version}\r\n");
    for line in lines.filter(|line| !line.is_empty()) {
        let name = line.split(':').next().unwrap_or_default().trim();
        let hop = [
            "connection",
            "keep-alive",
            "proxy-connection",
            "proxy-authorization",
        ]
        .iter()
        .any(|hop| name.eq_ignore_ascii_case(hop));
        if !hop {
            sent.push_str(line);
            sent.push_str("\r\n");
        }
    }
    sent.push_str("Connection: close\r\n\r\n");
    upstream.write_all(sent.as_bytes())?;
    splice(client, upstream, &early)
}

/// The request's head, through its blank line, and what came after it.
fn read_head(client: &mut TcpStream) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut read = Vec::new();
    let mut chunk = [0; 2048];
    loop {
        if let Some(end) = read.windows(4).position(|window| window == b"\r\n\r\n") {
            let early = read.split_off(end + 4);
            read.truncate(end);
            return Ok((read, early));
        }
        if read.len() > MAX_HEAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a head too long",
            ));
        }
        let count = client.read(&mut chunk)?;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        read.extend_from_slice(&chunk[..count]);
    }
}

/// `host:port` or `[v6]:port`.
fn authority(target: &str) -> Option<(String, u16)> {
    let (host, port) = match target.strip_prefix('[') {
        Some(rest) => {
            let (host, port) = rest.split_once("]:")?;
            (host, port)
        },
        None => target.rsplit_once(':')?,
    };
    let port = port.parse().ok().filter(|port| *port != 0)?;
    (!host.is_empty()).then(|| (host.to_owned(), port))
}

/// Looks `host` up and connects to one of its addresses, all of which must
/// be allowed; else the status to answer with.
fn connect(host: &str, port: u16, allow: Allow) -> Result<TcpStream, &'static str> {
    let addresses: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|_| "502 Bad Gateway")?
        .collect();
    if addresses.is_empty() {
        return Err("502 Bad Gateway");
    }
    tracing::debug!(host, port, "the PDF proxy looks a host up");
    if !addresses.iter().all(|address| allow(address.ip())) {
        tracing::debug!(host, "the PDF proxy refused a host off the public internet");
        return Err("403 Forbidden");
    }
    addresses
        .iter()
        .find_map(|address| TcpStream::connect_timeout(address, WAIT).ok())
        .ok_or("502 Bad Gateway")
}

fn refuse(client: &mut TcpStream, status: &str) -> io::Result<()> {
    client.write_all(
        format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes(),
    )
}

/// Copies both ways until either side ends or goes quiet for [`WAIT`],
/// then closes both.
fn splice(client: TcpStream, upstream: TcpStream, early: &[u8]) -> io::Result<()> {
    client.set_read_timeout(Some(WAIT))?;
    upstream.set_read_timeout(Some(WAIT))?;
    let mut upstream_out = upstream.try_clone()?;
    upstream_out.write_all(early)?;
    let mut client_in = client.try_clone()?;
    let (client_end, upstream_end) = (client.try_clone()?, upstream.try_clone()?);
    let outward = thread::Builder::new()
        .name("pdf-proxy-out".to_owned())
        .spawn(move || {
            let _ = io::copy(&mut client_in, &mut upstream_out);
            let _ = client_end.shutdown(Shutdown::Both);
            let _ = upstream_end.shutdown(Shutdown::Both);
        })?;
    let (mut upstream_in, mut client_out) = (upstream, client);
    let _ = io::copy(&mut upstream_in, &mut client_out);
    let _ = client_out.shutdown(Shutdown::Both);
    let _ = upstream_in.shutdown(Shutdown::Both);
    let _ = outward.join();
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Sends `request` through a proxy allowing `allow`, and reads all it
    /// answers.
    fn ask(allow: Allow, request: &str) -> String {
        let port = start(allow).unwrap();
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        answer
    }

    /// A server on this machine that answers each connection with `reply`
    /// after reading its first chunk, which it hands back.
    fn server(reply: &'static [u8]) -> (u16, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (heard, hear) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut chunk = [0; 4096];
            let count = stream.read(&mut chunk).unwrap();
            heard
                .send(String::from_utf8_lossy(&chunk[..count]).into_owned())
                .unwrap();
            stream.write_all(reply).unwrap();
        });
        (port, hear)
    }

    #[test]
    fn this_machine_and_its_network_are_refused_by_address_or_name() {
        for target in ["127.0.0.1:443", "localhost:443", "[::1]:443", "10.0.0.1:80"] {
            let answer = ask(
                moonframe_core::public_address,
                &format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n"),
            );
            assert!(answer.starts_with("HTTP/1.1 403"), "{target}: {answer}");
        }
        let answer = ask(
            moonframe_core::public_address,
            "GET http://localhost/ HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
        let answer = ask(moonframe_core::public_address, "nonsense\r\n\r\n");
        assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
    }

    #[test]
    fn an_allowed_host_is_tunnelled_to() {
        let (port, heard) = server(b"pong");
        let answer = ask(
            |_| true,
            &format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\nping"),
        );
        assert_eq!(answer, "HTTP/1.1 200 Connection Established\r\n\r\npong");
        assert_eq!(heard.recv().unwrap(), "ping");
    }

    #[test]
    fn a_plain_request_is_sent_on_in_origin_form() {
        let (port, heard) = server(b"HTTP/1.1 204 No Content\r\n\r\n");
        let answer = ask(
            |_| true,
            &format!(
                "GET http://127.0.0.1:{port}/a?b=1 HTTP/1.1\r\nHost: x\r\nProxy-Connection: \
                 keep-alive\r\n\r\n"
            ),
        );
        assert_eq!(answer, "HTTP/1.1 204 No Content\r\n\r\n");
        assert_eq!(
            heard.recv().unwrap(),
            "GET /a?b=1 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        );
    }
}
