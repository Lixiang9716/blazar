use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

pub fn http_response(status: u16, reason: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

pub fn spawn_one_shot_server<F>(handler: F) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(String) -> String + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut request_bytes = Vec::new();
        let mut header_end = None;
        let mut content_length = 0usize;

        loop {
            let mut buf = [0_u8; 4096];
            let n = stream.read(&mut buf).expect("read request");
            if n == 0 {
                break;
            }
            request_bytes.extend_from_slice(&buf[..n]);

            if header_end.is_none()
                && let Some(pos) = request_bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
            {
                header_end = Some(pos + 4);
                let headers = String::from_utf8_lossy(&request_bytes[..pos + 4]);
                content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
            }

            if let Some(end) = header_end
                && request_bytes.len() >= end + content_length
            {
                break;
            }
        }

        let request = String::from_utf8_lossy(&request_bytes).to_string();
        let response = handler(request);
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });
    (format!("http://{addr}"), handle)
}
