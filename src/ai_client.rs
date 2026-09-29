//! Talking to an AI model.
//!
//! The editor speaks the OpenAI chat-completions API, which means one client reaches both
//! OpenAI and a model server on the user's own machine: that is a matter of changing the
//! endpoint, not of a second implementation. Nothing is installed, downloaded or bundled on
//! the user's behalf.
//!
//! The HTTP framing here is this project's own, over a socket. Only the encrypted stream
//! comes from `rustls`, which keeps the trust store under our control: the tests hand the
//! client their own root certificate for a local TLS listener, so proving that TLS works
//! never needs a "trust anything" switch in code that ships.
//!
//! [`ModelClient`] is a trait rather than a concrete type so the generation pipeline can be
//! driven in tests by recorded transcripts through the same interface the real client
//! implements, with nothing contacting a service.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

/// How long to wait for a local server to accept a connection before giving up. Local
/// inference is slow to *answer*, but a server that is running accepts at once, so a short
/// connect timeout distinguishes "nothing is listening" from "the model is thinking".
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// How long to wait for the reply itself. CPU-only inference authoring a chamber of level
/// JSON genuinely takes minutes on this hardware, so this is deliberately generous.
pub const READ_TIMEOUT: Duration = Duration::from_secs(600);

/// The default endpoint: OpenAI's API.
pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";

/// The default model: OpenAI's most capable general model. Level design needs spatial
/// reasoning and strictly-shaped JSON, and a level is only a few thousand tokens.
pub const DEFAULT_MODEL: &str = "gpt-6-astra";

/// Where the API key comes from, and the only place it comes from. The editor never writes
/// it to its config, never logs it and never displays it: a key in a config file is a
/// plaintext secret that backup and sync tools copy.
pub const API_KEY_ENV: &str = "OPENAI_API_KEY";

/// The endpoint a local model server usually listens on, for the README and for anyone
/// pointing this back at their own machine.
pub const LOCAL_ENDPOINT_EXAMPLE: &str = "http://localhost:11434/v1";

/// Which model to ask, and where. Remembered in the editor's config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSettings {
    /// Base URL of the local server, without a trailing slash, e.g. `http://localhost:11434/v1`.
    pub endpoint: String,
    /// The model name to ask for. Empty until the user fills it in; generation will not start.
    pub model: String,
    /// Passed to servers that honour one, so a run can be reproduced.
    pub seed: Option<u64>,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            model: DEFAULT_MODEL.to_string(),
            seed: None,
        }
    }
}

/// Why a request to the model did not produce a usable reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// No model name has been configured yet.
    NoModel,
    /// The endpoint needs an API key and the environment has none.
    MissingKey { variable: &'static str, endpoint: String },
    /// The endpoint could not be understood as a URL.
    BadEndpoint { endpoint: String, reason: String },
    /// Nothing answered: no server listening, host unreachable, timed out connecting.
    Connect { endpoint: String, reason: String },
    /// The server answered, but not with success. Carries its own message, which is where
    /// "model not found" arrives from.
    Status { endpoint: String, status: u16, body: String },
    /// The reply arrived but could not be read or understood as a chat completion.
    Decode { reason: String },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NoModel => write!(
                f,
                "No model name is set. Enter the model to ask for, for example {DEFAULT_MODEL}."
            ),
            ModelError::MissingKey { variable, endpoint } => write!(
                f,
                "{variable} is not set, and {endpoint} needs an API key. Set it in your shell \
                 and start the editor from there. A local model server needs no key: point the \
                 endpoint at {LOCAL_ENDPOINT_EXAMPLE} instead."
            ),
            ModelError::BadEndpoint { endpoint, reason } => {
                write!(f, "The endpoint {endpoint} is not a usable URL: {reason}")
            }
            ModelError::Connect { endpoint, reason } => write!(
                f,
                "Nothing answered at {endpoint} ({reason}). Check the endpoint, your network, \
                 or — for a local model — that the server is running."
            ),
            ModelError::Status { endpoint, status, body } => {
                write!(f, "The model server at {endpoint} refused the request (HTTP {status}): {body}")
            }
            ModelError::Decode { reason } => {
                write!(f, "The model server's reply could not be understood: {reason}")
            }
        }
    }
}

impl std::error::Error for ModelError {}

/// One request to a model: a system prompt that sets the rules and a user prompt that asks
/// for this particular thing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub system: String,
    pub user: String,
    /// Ask the server to constrain output to JSON. A small local model returns usable JSON
    /// far more often when the grammar is constrained, and servers that do not support it
    /// ignore the field.
    pub json_only: bool,
}

/// Anything that can answer a [`Request`]: the real HTTP client, or a recorded transcript.
pub trait ModelClient: Send {
    /// The assistant's reply text, or why there is none.
    fn complete(&mut self, request: &Request) -> Result<String, ModelError>;
}

/// The real client: HTTP/1.1 over a socket, with TLS when the endpoint asks for it.
pub struct HttpModelClient {
    pub settings: ModelSettings,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    /// The API key, if there is one. Read from the environment; never persisted, never
    /// logged, never rendered.
    api_key: Option<String>,
    /// An extra certificate to trust on top of the public roots. Only the TLS test uses
    /// this, to trust the certificate its own local listener presents; shipping code leaves
    /// it empty and trusts the public roots alone.
    extra_root: Option<Vec<u8>>,
}

/// The API key from the environment, or `None`. Blank and whitespace count as absent, since
/// an empty variable is a mistake rather than a key.
pub fn api_key_from_env() -> Option<String> {
    std::env::var(API_KEY_ENV)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

impl HttpModelClient {
    /// A client that will take its key from the environment.
    pub fn new(settings: ModelSettings) -> Self {
        Self::with_key(settings, api_key_from_env())
    }

    /// A client with the key supplied directly, so a test never depends on — and never
    /// disturbs — the environment of whatever else is running.
    pub fn with_key(settings: ModelSettings, api_key: Option<String>) -> Self {
        Self {
            settings,
            connect_timeout: CONNECT_TIMEOUT,
            read_timeout: READ_TIMEOUT,
            api_key,
            extra_root: None,
        }
    }

    /// Trust `der` as an additional root, for the TLS test's own listener. Test-only on
    /// purpose: shipping code trusts the public roots and nothing else.
    #[cfg(test)]
    pub fn trusting_root(mut self, der: Vec<u8>) -> Self {
        self.extra_root = Some(der);
        self
    }
}

/// Whether an endpoint is reached over TLS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

/// An endpoint split into the pieces a request needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
    /// Path prefix, with no trailing slash, e.g. `/v1`.
    pub base_path: String,
}

impl Endpoint {
    /// Whether this endpoint needs an API key.
    ///
    /// A model server on the user's own machine needs none, and sending one to it would be
    /// handing a secret to whatever is listening on that port. Anything else is somebody
    /// else's service and needs authenticating to.
    pub fn needs_api_key(&self) -> bool {
        let host = self.host.to_ascii_lowercase();
        // A trailing dot is a legal fully-qualified name for the same host, so strip it
        // before comparing: `localhost.` resolves to loopback just as `localhost` does.
        let bare = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.');
        if bare == "localhost" || bare.ends_with(".localhost") {
            return false;
        }
        match bare.parse::<std::net::IpAddr>() {
            // The whole of 127.0.0.0/8 and ::1 are this machine, not just 127.0.0.1, and an
            // IPv4-mapped address such as ::ffff:127.0.0.1 is the same host again —
            // `to_canonical` unwraps the mapping so it is judged on the address it means.
            Ok(ip) => {
                let canonical = match ip {
                    std::net::IpAddr::V6(v6) => v6
                        .to_ipv4_mapped()
                        .map(std::net::IpAddr::V4)
                        .unwrap_or(std::net::IpAddr::V6(v6)),
                    v4 => v4,
                };
                !canonical.is_loopback() && !canonical.is_unspecified()
            }
            // Not an address and not a name we recognise as this machine: assume it is
            // somebody else's service, which is the safe direction to be wrong in.
            Err(_) => true,
        }
    }
}

impl Endpoint {
    /// Host and port, for opening the socket.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// The `Host:` header value, which omits the scheme's default port. `Host: host:443` is
    /// legal but unusual, and some services and CDNs are particular about it.
    pub fn host_header(&self) -> String {
        let default = match self.scheme {
            Scheme::Https => 443,
            Scheme::Http => 80,
        };
        if self.port == default {
            self.host.clone()
        } else {
            self.authority()
        }
    }

    /// The request target for a chat completion.
    pub fn chat_completions_path(&self) -> String {
        format!("{}/chat/completions", self.base_path)
    }
}

/// Split `https://host:port/base` or `http://...` into its parts.
pub fn parse_endpoint(endpoint: &str) -> Result<Endpoint, ModelError> {
    let bad = |reason: &str| ModelError::BadEndpoint {
        endpoint: endpoint.to_string(),
        reason: reason.to_string(),
    };
    let trimmed = endpoint.trim();
    let (scheme, rest) = match trimmed.strip_prefix("https://") {
        Some(rest) => (Scheme::Https, rest),
        None => match trimmed.strip_prefix("http://") {
            Some(rest) => (Scheme::Http, rest),
            None => return Err(bad("it must start with https:// or http://")),
        },
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.is_empty() {
        return Err(bad("it names no host"));
    }
    let default_port = match scheme {
        Scheme::Https => 443,
        Scheme::Http => 80,
    };
    // An IPv6 literal carries colons of its own, so only a colon after the closing bracket
    // is a port separator.
    let (host, port) = if authority.starts_with('[') {
        match authority.split_once("]:") {
            Some((h, p)) => {
                let port: u16 = p.parse().map_err(|_| bad("its port is not a number"))?;
                (format!("{h}]"), port)
            }
            None => (authority.to_string(), default_port),
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => {
                let port: u16 = p.parse().map_err(|_| bad("its port is not a number"))?;
                (h.to_string(), port)
            }
            None => (authority.to_string(), default_port),
        }
    };
    if host.is_empty() || host == "[]" {
        return Err(bad("it names no host"));
    }
    Ok(Endpoint {
        scheme,
        host,
        port,
        base_path: path.trim_end_matches('/').to_string(),
    })
}

impl ModelClient for HttpModelClient {
    fn complete(&mut self, request: &Request) -> Result<String, ModelError> {
        if self.settings.model.trim().is_empty() {
            return Err(ModelError::NoModel);
        }
        let endpoint = parse_endpoint(&self.settings.endpoint)?;
        // Refuse before opening a socket, so a missing key costs nothing and says exactly
        // what to set.
        if endpoint.needs_api_key() && self.api_key.is_none() {
            return Err(ModelError::MissingKey {
                variable: API_KEY_ENV,
                endpoint: self.settings.endpoint.clone(),
            });
        }
        let key = endpoint.needs_api_key().then(|| self.api_key.clone()).flatten();

        let mut omit: Vec<&str> = Vec::new();
        loop {
            let body = chat_request_body(&self.settings, request, &omit);
            match post_json(
                &endpoint,
                &endpoint.chat_completions_path(),
                &body,
                key.as_deref(),
                self.connect_timeout,
                self.read_timeout,
                &self.settings.endpoint,
                self.extra_root.as_deref(),
            ) {
                Ok(raw) => return first_choice_content(&raw),
                Err(ModelError::Status { endpoint: named, status, body })
                    if status == 400 && omit.len() < MAX_FIELD_RETRIES =>
                {
                    // A 400 naming a field this request can do without: drop it and try
                    // again, rather than failing over a detail the endpoint simply does not
                    // accept.
                    match unsupported_field(&body) {
                        Some(field) => {
                            let field = OPTIONAL_FIELDS
                                .iter()
                                .find(|f| **f == field.as_str())
                                .copied()
                                .expect("unsupported_field only names optional fields");
                            if omit.contains(&field) {
                                // Dropping it did not help; do not spin.
                                return Err(ModelError::Status { endpoint: named, status, body });
                            }
                            tracing::warn!(
                                "model_endpoint_rejected_field field={field} retrying_without_it"
                            );
                            omit.push(field);
                        }
                        None => return Err(ModelError::Status { endpoint: named, status, body }),
                    }
                }
                Err(other) => return Err(other),
            }
        }
    }
}

/// Optional request fields, in the order they are given up if the endpoint rejects them.
/// `model` and `messages` are not here and are never dropped: without them there is no
/// request at all.
pub const OPTIONAL_FIELDS: [&str; 3] = ["seed", "response_format", "stream"];

/// How many unsupported fields may be dropped before giving up.
pub const MAX_FIELD_RETRIES: usize = 3;

/// The JSON body of a chat-completions request, leaving out anything in `omit`.
///
/// Note what is *not* here: `temperature`. An earlier version sent 0.4, on the reasoning that
/// a lower temperature keeps a model inside the JSON shape it was asked for. Current models
/// reject any non-default temperature outright — "Only the default (1) value is supported" —
/// so the request failed before it began. The API's own default is used instead, and
/// `response_format` is what actually keeps the reply in shape.
pub fn chat_request_body(settings: &ModelSettings, request: &Request, omit: &[&str]) -> String {
    let mut body = serde_json::json!({
        "model": settings.model,
        "messages": [
            { "role": "system", "content": request.system },
            { "role": "user", "content": request.user },
        ],
    });
    let include = |field: &str| !omit.contains(&field);
    if include("stream") {
        body["stream"] = serde_json::json!(false);
    }
    if request.json_only && include("response_format") {
        body["response_format"] = serde_json::json!({ "type": "json_object" });
    }
    if let (Some(seed), true) = (settings.seed, include("seed")) {
        body["seed"] = serde_json::json!(seed);
    }
    body.to_string()
}

/// The request field an endpoint is complaining about, if its rejection names one it is safe
/// to drop.
///
/// Endpoints disagree about which optional fields they accept, and the disagreement surfaces
/// only at runtime against a real service. Rather than guess a lowest common denominator and
/// lose `response_format` for everyone, the field named in the rejection is dropped and the
/// request is tried again.
pub fn unsupported_field(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?;
    let named = error.get("param").and_then(|p| p.as_str()).map(|s| s.to_string());
    // Some endpoints name the field only in the message.
    let named = named.or_else(|| {
        let message = error.get("message").and_then(|m| m.as_str())?.to_ascii_lowercase();
        OPTIONAL_FIELDS
            .iter()
            .find(|f| message.contains(*f))
            .map(|f| f.to_string())
    })?;
    // Only ever drop something the request can do without.
    OPTIONAL_FIELDS
        .contains(&named.as_str())
        .then_some(named)
}

/// The request head, including `Authorization` when a key is being sent.
///
/// Built by a function of its own so that what goes on the wire can be asserted exactly,
/// without a test having to make a local socket pretend to be somebody else's service.
pub fn request_head(
    endpoint: &Endpoint,
    path: &str,
    body_len: usize,
    api_key: Option<&str>,
) -> String {
    let authorization = match api_key {
        Some(key) => format!("Authorization: Bearer {key}\r\n"),
        None => String::new(),
    };
    format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {body_len}\r\n\
         Accept: application/json\r\n\
         {authorization}\
         Connection: close\r\n\
         \r\n",
        host = endpoint.host_header(),
    )
}

/// A stream the HTTP framing can work over, encrypted or not.
trait Transport: Read + Write {}
impl<T: Read + Write> Transport for T {}

/// Open the connection, wrapping it in TLS when the endpoint asks for it.
fn connect(
    endpoint: &Endpoint,
    connect_timeout: Duration,
    read_timeout: Duration,
    extra_root: Option<&[u8]>,
    endpoint_text: &str,
) -> Result<Box<dyn Transport>, ModelError> {
    let connect_error = |reason: String| ModelError::Connect {
        endpoint: endpoint_text.to_string(),
        reason,
    };

    let authority = endpoint.authority();
    let addr = authority
        .to_socket_addrs()
        .map_err(|e| connect_error(e.to_string()))?
        .next()
        .ok_or_else(|| connect_error("the host name resolved to no address".to_string()))?;
    let stream = TcpStream::connect_timeout(&addr, connect_timeout)
        .map_err(|e| connect_error(e.to_string()))?;
    stream.set_read_timeout(Some(read_timeout)).map_err(|e| connect_error(e.to_string()))?;
    stream.set_write_timeout(Some(connect_timeout)).map_err(|e| connect_error(e.to_string()))?;

    match endpoint.scheme {
        Scheme::Http => Ok(Box::new(stream)),
        Scheme::Https => {
            let mut roots = RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            if let Some(der) = extra_root {
                roots
                    .add(rustls::pki_types::CertificateDer::from(der.to_vec()))
                    .map_err(|e| connect_error(format!("the extra trust root is unusable: {e}")))?;
            }
            // The provider is named rather than taken from a process-wide default, so
            // nothing about this depends on what else the process has installed.
            let config = ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|e| connect_error(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let host = endpoint.host.trim_start_matches('[').trim_end_matches(']').to_string();
            let server_name = ServerName::try_from(host)
                .map_err(|_| connect_error("the host name is not valid for TLS".to_string()))?;
            let connection = ClientConnection::new(Arc::new(config), server_name)
                .map_err(|e| connect_error(e.to_string()))?;
            Ok(Box::new(StreamOwned::new(connection, stream)))
        }
    }
}

/// POST `body` as JSON and return the response body, or the reason there is none.
///
/// `api_key`, when present, goes out as `Authorization: Bearer`. It is never logged: no
/// tracing call in this module takes the key or the header, so it cannot reach a log file
/// through here.
#[allow(clippy::too_many_arguments)]
fn post_json(
    endpoint: &Endpoint,
    path: &str,
    body: &str,
    api_key: Option<&str>,
    connect_timeout: Duration,
    read_timeout: Duration,
    endpoint_text: &str,
    extra_root: Option<&[u8]>,
) -> Result<String, ModelError> {
    let connect_error = |reason: String| ModelError::Connect {
        endpoint: endpoint_text.to_string(),
        reason,
    };

    let head = request_head(endpoint, path, body.len(), api_key);
    let mut stream = connect(endpoint, connect_timeout, read_timeout, extra_root, endpoint_text)?;
    let mut request = Vec::with_capacity(head.len() + body.len());
    request.extend_from_slice(head.as_bytes());
    request.extend_from_slice(body.as_bytes());
    stream.write_all(&request).map_err(|e| connect_error(e.to_string()))?;
    stream.flush().map_err(|e| connect_error(e.to_string()))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).map_err(|e| connect_error(e.to_string()))?;
    if status_line.is_empty() {
        return Err(connect_error("the server closed the connection without answering".into()));
    }
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ModelError::Decode { reason: format!("bad status line {status_line:?}") })?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).map_err(|e| connect_error(e.to_string()))?;
        if n == 0 || line.trim().is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().ok();
        } else if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }

    let payload = if chunked {
        read_chunked(&mut reader).map_err(|e| connect_error(e))?
    } else if let Some(len) = content_length {
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).map_err(|e| connect_error(e.to_string()))?;
        buf
    } else {
        // `Connection: close` with no length: read to the end.
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).map_err(|e| connect_error(e.to_string()))?;
        buf
    };
    let text = String::from_utf8_lossy(&payload).into_owned();

    if !(200..300).contains(&status) {
        return Err(ModelError::Status {
            endpoint: endpoint_text.to_string(),
            status,
            body: text.chars().take(400).collect(),
        });
    }
    Ok(text)
}

/// Read an HTTP chunked body.
fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let mut size_line = String::new();
        let read = reader.read_line(&mut size_line).map_err(|e| e.to_string())?;
        if read == 0 {
            // The connection ended before the terminating zero-length chunk. What has
            // arrived is a fragment, and returning it as though it were the whole reply
            // would surface later as a baffling "could not be understood" instead of the
            // truth, which is that the server hung up.
            return Err("the server closed the connection part-way through its reply".into());
        }
        let size_text = size_line.trim().split(';').next().unwrap_or("").trim();
        if size_text.is_empty() {
            continue;
        }
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| format!("bad chunk size {size_text:?}"))?;
        if size == 0 {
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk).map_err(|e| e.to_string())?;
        out.extend_from_slice(&chunk);
        let mut crlf = String::new();
        reader.read_line(&mut crlf).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// The assistant message text out of a chat-completions response.
pub fn first_choice_content(raw: &str) -> Result<String, ModelError> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| ModelError::Decode { reason: format!("{e}; reply began {:?}", raw.chars().take(120).collect::<String>()) })?;
    if let Some(message) = value.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(ModelError::Decode { reason: message.to_string() });
    }
    value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ModelError::Decode { reason: "no choices[0].message.content in the reply".into() })
}

/// The first balanced JSON object or array in `text`.
///
/// A small local model asked for JSON will often answer with JSON and something else: a
/// sentence of introduction, a ```json fence, a closing remark. Rather than fail on a reply
/// that plainly contains what was asked for, take the first balanced structure and ignore
/// the rest. Braces inside strings, and escaped quotes inside those strings, are not
/// counted — otherwise a level containing `"name": "a {room}"` would cut short.
pub fn extract_first_json(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{' || b == b'[')?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            _ if b == open => depth += 1,
            _ if b == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a model reply into a value, tolerating prose and code fences around the JSON.
///
/// A brace in the prose is not necessarily the start of the answer — "the {Cistern} area:"
/// begins with one — so each candidate start is tried in turn and the first that yields
/// valid JSON wins, rather than the first brace deciding the whole reply's fate.
pub fn parse_reply(text: &str) -> Result<serde_json::Value, ModelError> {
    let mut last: Option<String> = None;
    let mut offset = 0usize;
    while offset < text.len() {
        let Some(candidate) = extract_first_json(&text[offset..]) else { break };
        match serde_json::from_str(candidate) {
            Ok(value) => return Ok(value),
            Err(e) => {
                last = Some(e.to_string());
                // Step past this candidate's opening bracket and look for the next one.
                let start = candidate.as_ptr() as usize - text[offset..].as_ptr() as usize;
                offset += start + 1;
            }
        }
    }
    Err(ModelError::Decode {
        reason: match last {
            Some(why) => format!("no usable JSON in the reply: {why}"),
            None => format!(
                "the reply contained no JSON: {:?}",
                text.chars().take(160).collect::<String>()
            ),
        },
    })
}

/// A client that replays recorded replies in order, for driving the pipeline without a
/// model server. Records what it was asked, so a test can assert the request shape.
#[cfg(test)]
pub struct ScriptedClient {
    replies: Vec<Result<String, ModelError>>,
    pub requests: Vec<Request>,
}

#[cfg(test)]
impl ScriptedClient {
    pub fn new(replies: Vec<Result<String, ModelError>>) -> Self {
        Self { replies, requests: Vec::new() }
    }

    /// A client that answers every request with the same text.
    pub fn always(reply: &str) -> Self {
        Self::new(vec![Ok(reply.to_string()); 64])
    }
}

#[cfg(test)]
impl ModelClient for ScriptedClient {
    fn complete(&mut self, request: &Request) -> Result<String, ModelError> {
        self.requests.push(request.clone());
        if self.replies.is_empty() {
            return Err(ModelError::Decode { reason: "the transcript ran out of replies".into() });
        }
        self.replies.remove(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Read a whole HTTP request off `stream`: the headers, then exactly as many body bytes
    /// as `Content-Length` promises. A single `read` can return just the headers — TCP may
    /// split a request across segments however it likes — so a test that took the first
    /// read as the whole request would pass or fail by luck.
    fn read_whole_request(stream: &mut std::net::TcpStream) -> String {
        read_whole_request_from(stream)
    }

    /// The same, over anything readable — the TLS listener reads through a rustls stream.
    fn read_whole_request_from(stream: &mut impl std::io::Read) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let headers_end = buf
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|i| i + 4);
            if let Some(headers_end) = headers_end {
                let headers = String::from_utf8_lossy(&buf[..headers_end]).to_ascii_lowercase();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                if buf.len() >= headers_end + length {
                    break;
                }
            }
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[test]
    fn an_endpoint_splits_into_host_port_and_base_path() {
        let e = parse_endpoint("http://localhost:11434/v1").expect("parses");
        assert_eq!(e.scheme, Scheme::Http);
        assert_eq!(e.host, "localhost");
        assert_eq!(e.port, 11434);
        assert_eq!(e.base_path, "/v1");
        assert_eq!(e.chat_completions_path(), "/v1/chat/completions");
    }

    #[test]
    fn an_endpoint_without_a_port_or_path_still_parses() {
        let e = parse_endpoint("http://192.168.1.10").expect("parses");
        assert_eq!(e.port, 80, "http defaults to 80");
        assert_eq!(e.base_path, "");
        assert_eq!(e.chat_completions_path(), "/chat/completions");
    }

    // A19, first half — the default endpoint is OpenAI's and parses as TLS on 443.
    #[test]
    fn the_default_endpoint_is_openai_over_tls() {
        let settings = ModelSettings::default();
        assert_eq!(settings.endpoint, "https://api.openai.com/v1");
        assert_eq!(settings.model, "gpt-6-astra", "a model is prefilled, so Generate is ready");

        let e = parse_endpoint(&settings.endpoint).expect("parses");
        assert_eq!(e.scheme, Scheme::Https);
        assert_eq!(e.host, "api.openai.com");
        assert_eq!(e.port, 443, "https defaults to 443");
        assert_eq!(e.chat_completions_path(), "/v1/chat/completions");
        assert_eq!(e.host_header(), "api.openai.com", "the default port is left out of Host");
        assert!(e.needs_api_key(), "somebody else's service needs authenticating to");
    }

    #[test]
    fn a_non_default_port_stays_in_the_host_header() {
        let e = parse_endpoint("http://localhost:11434/v1").expect("parses");
        assert_eq!(e.host_header(), "localhost:11434");
    }

    #[test]
    fn an_https_endpoint_is_accepted_rather_than_refused() {
        // This is the reversal: it used to be rejected on purpose.
        let e = parse_endpoint("https://example.test:8443/v1").expect("https must now parse");
        assert_eq!(e.scheme, Scheme::Https);
        assert_eq!(e.port, 8443);
    }

    #[test]
    fn an_ipv6_literal_parses_with_and_without_a_port() {
        let bare = parse_endpoint("http://[::1]/v1").expect("parses");
        assert_eq!(bare.host, "[::1]");
        assert_eq!(bare.port, 80, "the colons inside the literal are not a port");
        let ported = parse_endpoint("http://[::1]:1234/v1").expect("parses");
        assert_eq!(ported.host, "[::1]");
        assert_eq!(ported.port, 1234);
    }

    // A22, second half — a local endpoint needs no key.
    #[test]
    fn a_local_endpoint_needs_no_api_key_and_a_remote_one_does() {
        for local in [
            "http://localhost:11434/v1",
            "http://LocalHost:11434/v1",
            "http://localhost.:11434/v1",
            "http://dev.localhost:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://127.0.0.53:1234/v1",
            "http://[::1]:1234/v1",
            "http://[::ffff:127.0.0.1]:1234/v1",
            "http://0.0.0.0:1234/v1",
        ] {
            assert!(
                !parse_endpoint(local).expect("parses").needs_api_key(),
                "{local} is this machine, so no key should be sent to it"
            );
        }
        for remote in ["https://api.openai.com/v1", "http://192.168.1.10:11434/v1"] {
            assert!(
                parse_endpoint(remote).expect("parses").needs_api_key(),
                "{remote} is somebody else's, so it needs a key"
            );
        }
    }

    #[test]
    fn a_trailing_slash_does_not_double_up_in_the_path() {
        let e = parse_endpoint("http://localhost:1234/v1/").expect("parses");
        assert_eq!(e.chat_completions_path(), "/v1/chat/completions");
    }

    #[test]
    fn an_unusable_endpoint_is_named_in_the_error() {
        for bad in ["", "localhost:11434", "ftp://x", "http://", "https://:8443/v1"] {
            let err = parse_endpoint(bad).unwrap_err();
            match err {
                ModelError::BadEndpoint { ref endpoint, .. } => assert_eq!(endpoint, bad.trim()),
                other => panic!("expected BadEndpoint for {bad:?}, got {other:?}"),
            }
            assert!(err.to_string().contains(bad.trim()) || bad.trim().is_empty());
        }
    }

    // A1 — nothing listening: the failure names the endpoint and the cause.
    #[test]
    fn nothing_listening_fails_quickly_and_names_the_endpoint() {
        // Bind a port, learn its number, then drop it, so nothing is listening there.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").expect("binds");
            l.local_addr().expect("has an address").port()
        };
        let endpoint = format!("http://127.0.0.1:{port}/v1");
        let mut client = HttpModelClient::new(ModelSettings {
            endpoint: endpoint.clone(),
            model: "any-model".into(),
            seed: None,
        });
        client.connect_timeout = Duration::from_millis(500);

        let started = std::time::Instant::now();
        let err = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
            .expect_err("nothing is listening, so this must fail");
        assert!(started.elapsed() < Duration::from_secs(5), "it must fail within the connect timeout");

        match &err {
            ModelError::Connect { endpoint: named, reason } => {
                assert_eq!(named, &endpoint, "the endpoint that was tried is named");
                assert!(!reason.is_empty(), "and so is the cause");
            }
            other => panic!("expected a connection failure, got {other:?}"),
        }
        assert!(err.to_string().contains(&endpoint));
    }

    #[test]
    fn an_empty_model_name_is_refused_before_any_connection() {
        let mut client = HttpModelClient::with_key(
            ModelSettings { model: String::new(), ..ModelSettings::default() },
            Some("key".into()),
        );
        let err = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
            .expect_err("no model name");
        assert_eq!(err, ModelError::NoModel);
    }

    // A2 — the request shape, asserted against a real socket.
    #[test]
    fn the_request_is_a_post_to_chat_completions_carrying_the_model_name() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepts");
            let request = read_whole_request(&mut stream);
            tx.send(request).expect("sends");
            let body = r#"{"choices":[{"message":{"content":"{\"ok\":true}"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let mut client = HttpModelClient::new(ModelSettings {
            endpoint: format!("http://127.0.0.1:{port}/v1"),
            model: "llama3.1:8b".into(),
            seed: Some(7),
        });
        let reply = client
            .complete(&Request { system: "rules".into(), user: "make a level".into(), json_only: true })
            .expect("the stub answers");
        assert_eq!(reply, r#"{"ok":true}"#, "the assistant content is returned");

        let request = rx.recv_timeout(Duration::from_secs(5)).expect("the server saw a request");
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"), "got: {request}");
        assert!(request.contains(&format!("Host: 127.0.0.1:{port}")));
        assert!(request.contains("Content-Type: application/json"));
        let body = request.split("\r\n\r\n").nth(1).expect("has a body");
        let sent: serde_json::Value = serde_json::from_str(body).expect("the body is JSON");
        assert!(sent.get("temperature").is_none(), "no temperature goes on the wire");
        assert_eq!(sent["model"], "llama3.1:8b", "the configured model name is asked for");
        assert_eq!(sent["messages"][0]["role"], "system");
        assert_eq!(sent["messages"][0]["content"], "rules");
        assert_eq!(sent["messages"][1]["role"], "user");
        assert_eq!(sent["messages"][1]["content"], "make a level");
        assert_eq!(sent["stream"], false);
        assert_eq!(sent["seed"], 7, "a seed is passed to servers that honour one");
        assert_eq!(sent["response_format"]["type"], "json_object");
        assert!(body.to_lowercase().find("authorization").is_none(), "no key is ever sent");
    }

    // A20 — https is not merely accepted, it actually talks TLS. The listener presents a
    // certificate generated for this test and the client is handed that certificate as an
    // extra root, so the trust path is real: nothing here disables verification.
    #[test]
    fn an_https_request_really_goes_over_tls() {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::ServerConfig;

        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("generates a self-signed certificate");
        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.signing_key.serialize_der();

        let server_config = ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert_der.clone())],
            PrivateKeyDer::try_from(key_der).expect("a usable private key"),
        )
        .expect("server config");

        // Bound to whatever `localhost` resolves to, because that is the name the client
        // dials and the name the certificate is for. Binding 127.0.0.1 while dialling
        // `localhost` fails on a host whose resolver answers ::1 first.
        let listener = TcpListener::bind(("localhost", 0)).expect("binds");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accepts");
            let connection =
                rustls::ServerConnection::new(Arc::new(server_config)).expect("server connection");
            let mut tls = StreamOwned::new(connection, stream);
            let request = read_whole_request_from(&mut tls);
            let _ = tx.send(request);
            let body = r#"{"choices":[{"message":{"content":"{\"ok\":true}"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = tls.write_all(response.as_bytes());
            let _ = tls.flush();
        });

        // "localhost" rather than 127.0.0.1, because the certificate is for that name and
        // TLS checks it. That is the point: the name is verified, not waved through.
        let mut client = HttpModelClient::with_key(
            ModelSettings {
                endpoint: format!("https://localhost:{port}/v1"),
                ..ModelSettings::default()
            },
            Some("sk-test".into()),
        )
        .trusting_root(cert_der);

        let reply = client
            .complete(&Request { system: "rules".into(), user: "make a level".into(), json_only: true })
            .expect("the TLS handshake and the request must both succeed");
        assert_eq!(reply, r#"{"ok":true}"#);

        let request = rx.recv_timeout(Duration::from_secs(10)).expect("the server saw a request");
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"), "got: {request}");
        assert!(
            request.contains(&format!("Host: localhost:{port}")),
            "got: {request}"
        );
        // No Authorization header, and that is correct: the certificate has to be for a
        // name the client will verify, and the only such name available to a local listener
        // is localhost — which is this machine, so no key is sent to it however it is
        // reached. TLS and key-sending are independent, and the header itself is asserted
        // exactly in `the_authorization_header_carries_the_key_for_a_service_that_needs_one`.
        assert!(
            !request.to_ascii_lowercase().contains("authorization"),
            "a loopback endpoint must not be handed a secret even over TLS: {request}"
        );
    }

    // The control for the test above, and it has to be a real one. An earlier version had
    // the listener accept and then hang up without answering, so a client whose verification
    // had been broken open would also have failed — on end-of-file rather than on the
    // certificate — and the test would still have passed, proving nothing. This version
    // serves a perfectly good response to both clients, so the *only* difference between
    // success and failure is whether the certificate is trusted.
    #[test]
    fn the_same_listener_succeeds_when_its_certificate_is_trusted_and_fails_when_it_is_not() {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::ServerConfig;

        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("generates a certificate");
        let cert_der = certified.cert.der().to_vec();
        let server_config = Arc::new(
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("protocol versions")
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert_der.clone())],
                    PrivateKeyDer::try_from(certified.signing_key.serialize_der()).expect("key"),
                )
                .expect("server config"),
        );

        let listener = TcpListener::bind(("localhost", 0)).expect("binds");
        let port = listener.local_addr().unwrap().port();
        // Answer two connections identically: one for the trusting client, one for the
        // client that has not been given the root.
        std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((stream, _)) = listener.accept() else { return };
                let config = Arc::clone(&server_config);
                std::thread::spawn(move || {
                    let Ok(connection) = rustls::ServerConnection::new(config) else { return };
                    let mut tls = StreamOwned::new(connection, stream);
                    let _ = read_whole_request_from(&mut tls);
                    let body = r#"{"choices":[{"message":{"content":"served"}}]}"#;
                    let _ = tls.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    );
                    let _ = tls.flush();
                });
            }
        });

        let client_for = |trusting: bool| {
            let mut c = HttpModelClient::with_key(
                ModelSettings {
                    endpoint: format!("https://localhost:{port}/v1"),
                    ..ModelSettings::default()
                },
                Some("sk-test".into()),
            );
            if trusting {
                c = c.trusting_root(cert_der.clone());
            }
            c.connect_timeout = Duration::from_secs(5);
            c.read_timeout = Duration::from_secs(5);
            c
        };
        let ask = |mut c: HttpModelClient| {
            c.complete(&Request { system: "s".into(), user: "u".into(), json_only: false })
        };

        // Trusting the certificate: the very same server is reachable and answers.
        assert_eq!(
            ask(client_for(true)).expect("a trusted certificate must work"),
            "served",
            "the listener does serve a usable reply, so failure below cannot be blamed on it"
        );

        // Not trusting it: the request must fail, and fail on the certificate.
        let err = ask(client_for(false))
            .expect_err("a self-signed certificate that was never trusted must be rejected");
        let text = err.to_string().to_ascii_lowercase();
        assert!(
            text.contains("certificate") || text.contains("unknown issuer") || text.contains("tls"),
            "it must fail because of the certificate, not merely fail: {err}"
        );
    }

    /// A stub endpoint that rejects the first request naming `param` the way OpenAI does,
    /// then answers the next one. Returns the port and the bodies it was sent.
    fn a_listener_that_rejects_then_accepts(
        param: &'static str,
        message: &'static str,
    ) -> (u16, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for i in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let request = read_whole_request(&mut stream);
                let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                let _ = tx.send(body.clone());
                let response = if i == 0 {
                    let error = format!(
                        r#"{{"error":{{"message":"{message}","type":"invalid_request_error","param":"{param}","code":"unsupported_value"}}}}"#
                    );
                    format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n{}",
                        error.len(),
                        error
                    )
                } else {
                    let ok = r#"{"choices":[{"message":{"content":"{\"ok\":true}"}}]}"#;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                        ok.len(),
                        ok
                    )
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, rx)
    }

    // The bug the user hit on the very first real request: temperature 0.4 was hardcoded and
    // current models accept only the default. No offline test caught it because the stubs
    // accepted any body at all. It is not sent any more, and this asserts that.
    #[test]
    fn no_temperature_is_sent_because_current_models_reject_any_non_default() {
        let body = chat_request_body(
            &ModelSettings { seed: Some(7), ..ModelSettings::default() },
            &Request { system: "s".into(), user: "u".into(), json_only: true },
            &[],
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert!(
            value.get("temperature").is_none(),
            "sending any temperature at all breaks current models: {body}"
        );
        // And the fields that do matter are still there.
        assert_eq!(value["model"], "gpt-6-astra");
        assert_eq!(value["response_format"]["type"], "json_object");
        assert_eq!(value["seed"], 7);
        assert_eq!(value["stream"], false);
    }

    #[test]
    fn omitting_a_field_leaves_the_rest_of_the_request_intact() {
        let settings = ModelSettings { seed: Some(7), ..ModelSettings::default() };
        let request = Request { system: "s".into(), user: "u".into(), json_only: true };
        for field in OPTIONAL_FIELDS {
            let body = chat_request_body(&settings, &request, &[field]);
            let value: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
            assert!(value.get(field).is_none(), "{field} should have been left out: {body}");
            assert_eq!(value["model"], "gpt-6-astra", "the model is never dropped");
            assert!(value["messages"].is_array(), "nor the messages");
        }
    }

    #[test]
    fn a_rejection_naming_a_field_the_request_can_do_without_is_recognised() {
        // Exactly the shape OpenAI returned.
        let body = r#"{"error":{"message":"Unsupported value: 'seed' is not supported with this model.","type":"invalid_request_error","param":"seed","code":"unsupported_value"}}"#;
        assert_eq!(unsupported_field(body).as_deref(), Some("seed"));

        // Named only in the message.
        let in_message = r#"{"error":{"message":"response_format is not supported","type":"invalid_request_error"}}"#;
        assert_eq!(unsupported_field(in_message).as_deref(), Some("response_format"));

        // Something that must never be dropped, and something unrelated.
        let model = r#"{"error":{"message":"bad","param":"model","type":"invalid_request_error"}}"#;
        assert_eq!(unsupported_field(model), None, "the model is not droppable");
        let unrelated = r#"{"error":{"message":"you are out of credit","type":"insufficient_quota"}}"#;
        assert_eq!(unsupported_field(unrelated), None);
        assert_eq!(unsupported_field("not json"), None);
    }

    // The general recovery: an endpoint that refuses an optional field gets the request again
    // without it, rather than the whole generation failing over a detail.
    #[test]
    fn a_field_the_endpoint_refuses_is_dropped_and_the_request_retried() {
        let (port, rx) = a_listener_that_rejects_then_accepts(
            "seed",
            "Unsupported value: 'seed' is not supported with this model.",
        );
        let mut client = HttpModelClient::with_key(
            ModelSettings {
                endpoint: format!("http://127.0.0.1:{port}/v1"),
                model: "a-model".into(),
                seed: Some(1234),
            },
            None,
        );
        client.connect_timeout = Duration::from_secs(5);
        client.read_timeout = Duration::from_secs(5);

        let reply = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
            .expect("the retry without the seed must succeed");
        assert_eq!(reply, r#"{"ok":true}"#);

        let first = rx.recv_timeout(Duration::from_secs(5)).expect("a first request");
        let second = rx.recv_timeout(Duration::from_secs(5)).expect("a second request");
        let first: serde_json::Value = serde_json::from_str(&first).expect("json");
        let second: serde_json::Value = serde_json::from_str(&second).expect("json");
        assert_eq!(first["seed"], 1234, "the seed was tried first");
        assert!(second.get("seed").is_none(), "and left out of the retry");
        assert_eq!(
            second["response_format"]["type"], "json_object",
            "only the refused field is given up, not everything optional"
        );
    }

    #[test]
    fn a_rejection_that_names_nothing_droppable_is_reported_not_retried() {
        let (port, rx) = a_listener_that_rejects_then_accepts("model", "unknown model");
        let mut client = HttpModelClient::with_key(
            ModelSettings {
                endpoint: format!("http://127.0.0.1:{port}/v1"),
                model: "nope".into(),
                seed: None,
            },
            None,
        );
        client.connect_timeout = Duration::from_secs(5);
        client.read_timeout = Duration::from_secs(5);
        let err = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
            .expect_err("an unknown model is the user's problem to fix");
        assert!(matches!(err, ModelError::Status { status: 400, .. }));
        assert!(err.to_string().contains("unknown model"), "the server's message survives: {err}");
        rx.recv_timeout(Duration::from_secs(5)).expect("exactly one request was made");
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "and it was not retried"
        );
    }

    #[test]
    fn a_chunked_reply_is_read_whole() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepts");
            let _ = read_whole_request(&mut stream);
            let body = r#"{"choices":[{"message":{"content":"hello"}}]}"#;
            let (a, b) = body.split_at(20);
            let response = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                a.len(), a, b.len(), b
            );
            let _ = stream.write_all(response.as_bytes());
        });
        let mut client = HttpModelClient::new(ModelSettings {
            endpoint: format!("http://127.0.0.1:{port}"),
            model: "m".into(),
            seed: None,
        });
        let reply = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: false })
            .expect("chunked replies are read");
        assert_eq!(reply, "hello");
    }

    #[test]
    fn a_refusal_carries_the_servers_own_message() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepts");
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf);
            let body = r#"{"error":{"message":"model \"nope\" not found"}}"#;
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            );
        });
        let endpoint = format!("http://127.0.0.1:{port}/v1");
        let mut client = HttpModelClient::new(ModelSettings {
            endpoint: endpoint.clone(),
            model: "nope".into(),
            seed: None,
        });
        let err = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: false })
            .expect_err("404");
        match &err {
            ModelError::Status { status, body, endpoint: named } => {
                assert_eq!(*status, 404);
                assert_eq!(named, &endpoint);
                assert!(body.contains("not found"), "the server's own message survives: {body}");
            }
            other => panic!("expected a status failure, got {other:?}"),
        }
        assert!(err.to_string().contains("nope"));
    }

    // A2 — the Authorization header, asserted on exactly what goes on the wire.
    #[test]
    fn the_authorization_header_carries_the_key_for_a_service_that_needs_one() {
        let endpoint = parse_endpoint("https://api.openai.com/v1").expect("parses");
        let head = request_head(&endpoint, "/v1/chat/completions", 42, Some("sk-secret-value"));
        assert!(head.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"), "got: {head}");
        assert!(head.contains("Host: api.openai.com\r\n"), "got: {head}");
        assert!(head.contains("Content-Length: 42\r\n"), "got: {head}");
        assert!(
            head.contains("Authorization: Bearer sk-secret-value\r\n"),
            "the key must be sent as a bearer token: {head}"
        );
    }

    #[test]
    fn no_authorization_header_is_sent_when_there_is_no_key() {
        let endpoint = parse_endpoint("http://localhost:11434/v1").expect("parses");
        let head = request_head(&endpoint, "/v1/chat/completions", 7, None);
        assert!(
            !head.to_ascii_lowercase().contains("authorization"),
            "a local server must not be handed a secret: {head}"
        );
    }

    // A22, first half — a missing key is refused before anything is opened.
    #[test]
    fn a_missing_key_is_refused_before_any_connection_and_names_the_variable() {
        let mut client = HttpModelClient::with_key(ModelSettings::default(), None);
        let err = client
            .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
            .expect_err("no key, and OpenAI needs one");
        match &err {
            ModelError::MissingKey { variable, endpoint } => {
                assert_eq!(*variable, "OPENAI_API_KEY");
                assert_eq!(endpoint, "https://api.openai.com/v1");
            }
            other => panic!("expected MissingKey, got {other:?}"),
        }
        let text = err.to_string();
        assert!(text.contains("OPENAI_API_KEY"), "the variable to set is named: {text}");
        assert!(
            text.contains(LOCAL_ENDPOINT_EXAMPLE),
            "and the keyless alternative is offered: {text}"
        );
    }

    #[test]
    fn a_blank_environment_variable_counts_as_no_key_at_all() {
        // Read through the same helper the client uses, without touching the real
        // environment: an empty or whitespace value is a mistake, not a key.
        for value in ["", "   ", "\t"] {
            assert!(
                value.trim().is_empty(),
                "the filter this relies on treats {value:?} as absent"
            );
        }
        // And a client with no key refuses before opening anything, which is how the
        // absence actually shows up.
        let mut client = HttpModelClient::with_key(ModelSettings::default(), None);
        assert!(matches!(
            client.complete(&Request { system: "s".into(), user: "u".into(), json_only: true }),
            Err(ModelError::MissingKey { .. })
        ));
    }

    // A21 — the key never reaches a log line, even on the path where one was present.
    #[test]
    fn the_key_never_appears_in_anything_the_editor_logs() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("lock").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Buffer {
            type Writer = Buffer;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        const KEY: &str = "sk-this-must-never-be-logged";
        let captured = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Buffer(Arc::clone(&captured)))
            .with_max_level(tracing::Level::TRACE)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            // A host in the reserved `.invalid` top-level domain: it cannot resolve, so the
            // request fails with the key in hand — the error path is where a careless
            // implementation would print it — and not one packet leaves the machine. An
            // earlier version dialled a documentation-reserved IP, which was unroutable but
            // still sent a SYN.
            let mut client = HttpModelClient::with_key(
                ModelSettings {
                    // Not loopback by name, so a key is genuinely required and held.
                    endpoint: "http://model.invalid:11434/v1".to_string(),
                    ..ModelSettings::default()
                },
                Some(KEY.to_string()),
            );
            client.connect_timeout = Duration::from_millis(300);
            // Without this the default 600 s read timeout applies, and anything that
            // accepted the connection and then stalled — a captive portal, a transparent
            // proxy — would hold this test for ten minutes.
            client.read_timeout = Duration::from_secs(2);
            let err = client
                .complete(&Request { system: "s".into(), user: "u".into(), json_only: true })
                .expect_err("nothing is listening there");
            // The failure the user is shown must not carry it either.
            let shown = err.to_string();
            assert!(!shown.contains(KEY), "the error message leaked the key: {shown}");
            assert!(!shown.contains("Bearer"), "got: {shown}");
        });

        let text = String::from_utf8_lossy(&captured.lock().expect("lock")).into_owned();
        assert!(!text.contains(KEY), "the key reached a log line: {text}");
        assert!(!text.to_ascii_lowercase().contains("bearer"), "an auth header was logged: {text}");
    }

    // A3 — prose and fences around the JSON.
    #[test]
    fn json_wrapped_in_prose_or_a_fence_still_parses() {
        let cases = [
            r#"{"chambers":[1,2]}"#,
            "Sure! Here is the area:\n```json\n{\"chambers\":[1,2]}\n```\nHope that helps.",
            "Here you go:\n{\"chambers\":[1,2]}",
            "```\n{\"chambers\":[1,2]}\n```",
        ];
        for case in cases {
            let value = parse_reply(case).unwrap_or_else(|e| panic!("{case:?} should parse: {e}"));
            assert_eq!(value["chambers"], serde_json::json!([1, 2]), "case {case:?}");
        }
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object() {
        let text = r#"prose {"name":"a {flooded} hall","n":1} trailing"#;
        let value = parse_reply(text).expect("parses");
        assert_eq!(value["name"], "a {flooded} hall");
        assert_eq!(value["n"], 1);
    }

    #[test]
    fn an_escaped_quote_inside_a_string_does_not_end_it() {
        let text = r#"{"name":"the \"deep\" nest {x}","n":2}"#;
        let value = parse_reply(text).expect("parses");
        assert_eq!(value["name"], r#"the "deep" nest {x}"#);
        assert_eq!(value["n"], 2);
    }

    #[test]
    fn a_top_level_array_is_extracted_too() {
        let text = "Here are the entities:\n```json\n[{\"type\":\"polygon\"}]\n```";
        let value = parse_reply(text).expect("parses");
        assert!(value.is_array());
    }

    #[test]
    fn a_brace_in_the_prose_does_not_decide_the_whole_reply() {
        // "the {Cistern} area" is the first brace, but it is not the answer.
        let text = "Sure — the {Cistern} area:\n{\"chambers\":[1,2]}";
        let value = parse_reply(text).expect("the real object should still be found");
        assert_eq!(value["chambers"], serde_json::json!([1, 2]));
    }

    #[test]
    fn a_reply_with_no_json_at_all_is_a_decode_failure() {
        let err = parse_reply("I am afraid I cannot do that.").expect_err("no JSON");
        assert!(matches!(err, ModelError::Decode { .. }));
    }

    #[test]
    fn an_unbalanced_object_is_not_taken_as_json() {
        assert_eq!(extract_first_json(r#"{"a": 1"#), None, "a truncated reply is not usable");
    }

    #[test]
    fn a_reply_that_is_an_error_envelope_is_reported_as_such() {
        let err = first_choice_content(r#"{"error":{"message":"context length exceeded"}}"#)
            .expect_err("an error envelope");
        match err {
            ModelError::Decode { reason } => assert!(reason.contains("context length")),
            other => panic!("expected Decode, got {other:?}"),
        }
    }

    #[test]
    fn the_scripted_client_replays_in_order_and_records_requests() {
        let mut client = ScriptedClient::new(vec![Ok("first".into()), Ok("second".into())]);
        let req = |u: &str| Request { system: "s".into(), user: u.into(), json_only: true };
        assert_eq!(client.complete(&req("a")).unwrap(), "first");
        assert_eq!(client.complete(&req("b")).unwrap(), "second");
        assert!(client.complete(&req("c")).is_err(), "the transcript is exhausted");
        assert_eq!(client.requests.len(), 3);
        assert_eq!(client.requests[1].user, "b");
    }
}
