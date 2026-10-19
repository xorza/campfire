use std::fmt::Write as _;
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use campfire_common::ErrorReport;
use campfire_store::Worker;

const SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";

/// One SOAP action the fake router took: its name, and each argument's name and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Call {
    pub(crate) action: String,
    args: Vec<(String, String)>,
}

/// What the fake router answers an action with: success, or a UPnP error code.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Answer {
    Done,
    Error(u16),
}

/// A UPnP router on loopback: it answers the SSDP search on its own port, serves its device and
/// service descriptions, and answers each SOAP action with what `answer` gives, keeping each
/// call. Its service offers `AddPortMapping`, `DeletePortMapping` and `GetExternalIPAddress`, and
/// `AddAnyPortMapping` too when it starts so, which reserves the port asked for with its lowest
/// bit flipped.
pub(crate) struct FakeRouter {
    ssdp: SocketAddr,
    http_port: u16,
    calls: Arc<Mutex<Vec<Call>>>,
    stop: Arc<AtomicBool>,
    _workers: [Worker; 2],
}

impl Call {
    pub(crate) fn arg(&self, name: &str) -> &str {
        &self
            .args
            .iter()
            .find(|(arg, _)| arg == name)
            .unwrap_or_else(|| panic!("{} takes {name}", self.action))
            .1
    }

    fn of(action: &str, body: &str) -> Self {
        let mut rest = body
            .split_once(&format!("{action} xmlns"))
            .and_then(|(_, rest)| rest.split_once('>'))
            .map_or("", |(_, rest)| rest);
        let mut args = Vec::new();
        while let Some((_, after)) = rest.split_once('<') {
            let Some((name, after)) = after.split_once('>') else {
                break;
            };
            if name.starts_with('/') {
                break;
            }
            let (value, after) = after.split_once('<').unwrap();
            let after = after.split_once('>').unwrap().1;
            args.push((name.to_owned(), value.to_owned()));
            rest = after;
        }
        Self {
            action: action.to_owned(),
            args,
        }
    }
}

impl FakeRouter {
    /// The address the router says it is reached at outside.
    pub(crate) const EXTERNAL_IP: Ipv4Addr = Ipv4Addr::new(203, 0, 113, 7);

    /// Where the router answers a search.
    pub(crate) const fn ssdp(&self) -> SocketAddr {
        self.ssdp
    }

    pub(crate) fn start(answer: impl FnMut(&Call) -> Answer + Send + 'static) -> Self {
        Self::launch(false, answer)
    }

    pub(crate) fn offering_add_any(answer: impl FnMut(&Call) -> Answer + Send + 'static) -> Self {
        Self::launch(true, answer)
    }

    fn launch(add_any: bool, mut answer: impl FnMut(&Call) -> Answer + Send + 'static) -> Self {
        let http = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        http.set_nonblocking(true).unwrap();
        let http_port = http.local_addr().unwrap().port();
        let ssdp = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        ssdp.set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        let ssdp_address = ssdp.local_addr().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let searcher = {
            let stop = Arc::clone(&stop);
            Worker::start("fake-ssdp", move || {
                let mut buffer = [0; 1500];
                while !stop.load(Ordering::Relaxed) {
                    if let Ok((_, from)) = ssdp.recv_from(&mut buffer) {
                        let reply = format!(
                            "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\nST: \
                             urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\nUSN: \
                             uuid:fake\r\nLOCATION: http://127.0.0.1:{http_port}/desc.xml\r\n\r\n"
                        );
                        ssdp.send_to(reply.as_bytes(), from).unwrap();
                    }
                }
            })
        };
        let server = {
            let (calls, stop) = (Arc::clone(&calls), Arc::clone(&stop));
            Worker::start("fake-igd", move || {
                while !stop.load(Ordering::Relaxed) {
                    match http.accept() {
                        Ok((stream, _)) => {
                            Self::serve(stream, add_any, &mut |call| {
                                let answer = answer(&call);
                                calls.lock().unwrap().push(call);
                                answer
                            });
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!(
                            "the fake router's listener failed: {}",
                            ErrorReport::of(&error)
                        ),
                    }
                }
            })
        };
        Self {
            ssdp: ssdp_address,
            http_port,
            calls,
            stop,
            _workers: [searcher, server],
        }
    }

    /// Answers one HTTP request on `stream`: a description, or a SOAP action.
    fn serve(mut stream: TcpStream, add_any: bool, answer: &mut impl FnMut(Call) -> Answer) {
        stream.set_nonblocking(false).unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        let (head, body_len) = loop {
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
            let text = String::from_utf8_lossy(&request);
            if let Some((head, _)) = text.split_once("\r\n\r\n") {
                let body_len = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                break (head.to_owned(), body_len);
            }
        };
        while request.len() < head.len() + 4 + body_len {
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
        }
        let body = String::from_utf8_lossy(&request[head.len() + 4..]).into_owned();
        let path = head.split(' ').nth(1).unwrap();
        let (status, content) = match path {
            "/desc.xml" => (200, Self::description()),
            "/scpd.xml" => (200, Self::service(add_any)),
            "/ctl" => {
                let action = head
                    .lines()
                    .find_map(|line| line.split_once('#'))
                    .map(|(_, action)| action.trim_end_matches('"'))
                    .unwrap()
                    .to_owned();
                let call = Call::of(&action, &body);
                let reserved = (action == "AddAnyPortMapping")
                    .then(|| call.arg("NewExternalPort").parse::<u16>().unwrap() ^ 1);
                match answer(call) {
                    Answer::Done => {
                        let out = match (action.as_str(), reserved) {
                            ("GetExternalIPAddress", _) => format!(
                                "<NewExternalIPAddress>{}</NewExternalIPAddress>",
                                Self::EXTERNAL_IP
                            ),
                            (_, Some(port)) => {
                                format!("<NewReservedPort>{port}</NewReservedPort>")
                            }
                            _ => String::new(),
                        };
                        (
                            200,
                            Self::envelope(&format!(
                                r#"<u:{action}Response xmlns:u="{SERVICE}">{out}</u:{action}Response>"#
                            )),
                        )
                    }
                    Answer::Error(code) => (
                        500,
                        Self::envelope(&format!(
                            r#"<s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail><UPnPError xmlns="urn:schemas-upnp-org:control-1-0"><errorCode>{code}</errorCode><errorDescription>refused</errorDescription></UPnPError></detail></s:Fault>"#
                        )),
                    ),
                }
            }
            path => panic!("no resource at {path}"),
        };
        let reply = format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: \
             close\r\n\r\n{content}",
            content.len()
        );
        stream.write_all(reply.as_bytes()).unwrap();
    }

    fn envelope(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>{body}</s:Body></s:Envelope>"#
        )
    }

    fn description() -> String {
        format!(
            r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0"><device><deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType><serviceList><service><serviceType>{SERVICE}</serviceType><serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId><SCPDURL>/scpd.xml</SCPDURL><controlURL>/ctl</controlURL><eventSubURL>/evt</eventSubURL></service></serviceList></device></root>"#
        )
    }

    fn service(add_any: bool) -> String {
        let action = |name: &str, ins: &[&str]| {
            let mut args = String::new();
            for arg in ins {
                write!(
                    args,
                    "<argument><name>{arg}</name><direction>in</direction></argument>"
                )
                .unwrap();
            }
            format!("<action><name>{name}</name><argumentList>{args}</argumentList></action>")
        };
        let mapping = [
            "NewRemoteHost",
            "NewExternalPort",
            "NewProtocol",
            "NewInternalPort",
            "NewInternalClient",
            "NewEnabled",
            "NewPortMappingDescription",
            "NewLeaseDuration",
        ];
        let add_any = if add_any {
            action("AddAnyPortMapping", &mapping)
        } else {
            String::new()
        };
        format!(
            r#"<?xml version="1.0"?><scpd xmlns="urn:schemas-upnp-org:service-1-0"><actionList>{}{}{}{add_any}</actionList></scpd>"#,
            action("AddPortMapping", &mapping),
            action(
                "DeletePortMapping",
                &["NewRemoteHost", "NewExternalPort", "NewProtocol"]
            ),
            action("GetExternalIPAddress", &[]),
        )
    }

    /// Its `NOTIFY` announcement that it is alive, at its description's location.
    pub(crate) fn announcement(&self) -> Vec<u8> {
        format!(
            "NOTIFY * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nCACHE-CONTROL: max-age=1800\r\n\
             LOCATION: http://127.0.0.1:{}/desc.xml\r\nNT: \
             urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\nNTS: ssdp:alive\r\nUSN: \
             uuid:fake::urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n",
            self.http_port
        )
        .into_bytes()
    }

    pub(crate) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

impl Drop for FakeRouter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
