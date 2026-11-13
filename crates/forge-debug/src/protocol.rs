//! DBGp wire format and typed views of the engine's answers.
//!
//! IDE → engine: `command -i <tid> -x "arg" -- <base64 data>\0`.
//! Engine → IDE: `<length>\0<xml>\0`.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt};

use crate::error::{DebugError, DebugResult};
use crate::xml::Element;

pub fn b64_encode(s: &str) -> String {
    B64.encode(s.as_bytes())
}

pub fn b64_decode(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    match B64.decode(clean.as_bytes()) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => s.to_string(),
    }
}

/// Quote an argument value when it contains spaces, quotes or is empty.
fn quote_arg(v: &str) -> String {
    if !v.is_empty() && !v.contains([' ', '"', '\\', '\0', '\t', '\n']) {
        return v.to_string();
    }
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Encode one command, including the trailing NUL byte.
pub fn encode_command(
    name: &str,
    tid: u32,
    args: &[(char, String)],
    data: Option<&str>,
) -> Vec<u8> {
    let mut s = format!("{name} -i {tid}");
    for (flag, value) in args {
        s.push_str(&format!(" -{flag} {}", quote_arg(value)));
    }
    if let Some(data) = data {
        s.push_str(" -- ");
        s.push_str(&b64_encode(data));
    }
    let mut bytes = s.into_bytes();
    bytes.push(0);
    bytes
}

/// A decoded command, as seen by the engine (used by tests and the fake client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub tid: u32,
    pub args: Vec<(char, String)>,
    pub data: Option<String>,
}

impl Command {
    pub fn arg(&self, flag: char) -> Option<&str> {
        self.args
            .iter()
            .find(|(f, _)| *f == flag)
            .map(|(_, v)| v.as_str())
    }
}

/// Parse a command line produced by [`encode_command`] (without the NUL byte).
pub fn parse_command(line: &str) -> DebugResult<Command> {
    let (head, data) = match line.split_once(" -- ") {
        Some((h, d)) => (h, Some(b64_decode(d.trim()))),
        None => (line, None),
    };
    let mut chars = head.chars().peekable();
    let mut tokens = Vec::new();
    while chars.peek().is_some() {
        while chars.peek() == Some(&' ') {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut tok = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            tok.push(n)
                        }
                    }
                    '"' => break,
                    c => tok.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ' ' {
                    break;
                }
                tok.push(c);
                chars.next();
            }
        }
        tokens.push(tok);
    }
    let mut it = tokens.into_iter();
    let name = it
        .next()
        .ok_or_else(|| DebugError::Protocol("empty command".into()))?;
    let mut args = Vec::new();
    let mut tid = 0;
    while let Some(flag) = it.next() {
        let f = flag
            .strip_prefix('-')
            .and_then(|f| f.chars().next())
            .ok_or_else(|| DebugError::Protocol(format!("bad flag {flag}")))?;
        let value = it.next().unwrap_or_default();
        if f == 'i' {
            tid = value.parse().unwrap_or(0);
        } else {
            args.push((f, value));
        }
    }
    Ok(Command {
        name,
        tid,
        args,
        data,
    })
}

/// Longest length prefix accepted: a decimal `usize` plus the NUL.
const MAX_LENGTH_HEADER: u64 = 21;

/// Largest packet body accepted. The length comes from the peer (any host that can reach
/// the listener), so it must not size an allocation unchecked. Xdebug truncates values to
/// `max_data`, so real packets stay far below this.
pub const MAX_PACKET_LEN: usize = 64 * 1024 * 1024;

/// Read one engine packet (`<len>\0<xml>\0`). `None` at end of stream.
pub async fn read_packet<R: AsyncBufRead + Unpin>(r: &mut R) -> DebugResult<Option<String>> {
    let mut len_buf = Vec::new();
    let n = (&mut *r)
        .take(MAX_LENGTH_HEADER)
        .read_until(0, &mut len_buf)
        .await?;
    if n == 0 {
        return Ok(None);
    }
    if len_buf.last() != Some(&0) {
        if n as u64 == MAX_LENGTH_HEADER {
            return Err(DebugError::Protocol("packet length header too long".into()));
        }
        return Ok(None);
    }
    len_buf.pop();
    let len: usize = std::str::from_utf8(&len_buf)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| DebugError::Protocol("bad packet length".into()))?;
    if len > MAX_PACKET_LEN {
        return Err(DebugError::Protocol(format!(
            "packet of {len} bytes exceeds the {MAX_PACKET_LEN} byte limit"
        )));
    }
    // Grow with the bytes that actually arrive instead of trusting the announced length.
    let mut body = Vec::new();
    (&mut *r).take(len as u64).read_to_end(&mut body).await?;
    if body.len() != len {
        return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
    }
    let mut nul = [0u8; 1];
    r.read_exact(&mut nul).await?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

/// Frame an XML message as the engine sends it (used by the fake client in tests).
pub fn frame_packet(xml: &str) -> Vec<u8> {
    let mut out = format!("{}\0", xml.len()).into_bytes();
    out.extend_from_slice(xml.as_bytes());
    out.push(0);
    out
}

/// The engine's first packet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitPacket {
    pub app_id: String,
    pub ide_key: String,
    pub session: String,
    pub thread: String,
    pub language: String,
    pub protocol_version: String,
    pub file_uri: String,
    pub engine_name: String,
    pub engine_version: String,
}

impl InitPacket {
    pub fn parse(el: &Element) -> DebugResult<Self> {
        if el.local_name() != "init" {
            return Err(DebugError::Protocol(format!(
                "expected <init>, got <{}>",
                el.name
            )));
        }
        let a = |n: &str| el.attr(n).unwrap_or_default().to_string();
        let engine = el.child("engine");
        Ok(Self {
            app_id: a("appid"),
            ide_key: a("idekey"),
            session: a("session"),
            thread: a("thread"),
            language: a("language"),
            protocol_version: a("protocol_version"),
            file_uri: a("fileuri"),
            engine_name: engine
                .map(|e| e.text.trim().to_string())
                .unwrap_or_default(),
            engine_version: engine
                .and_then(|e| e.attr("version"))
                .unwrap_or_default()
                .to_string(),
        })
    }
}

/// Fail with the engine's error if the response carries one.
pub fn check_error(el: &Element) -> DebugResult<()> {
    if let Some(err) = el.child("error") {
        let code = err.attr_u32("code").unwrap_or(0);
        let message = err
            .child("message")
            .map(|m| m.text.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| default_error_message(code).to_string());
        return Err(DebugError::Engine { code, message });
    }
    Ok(())
}

fn default_error_message(code: u32) -> &'static str {
    match code {
        1 => "parse error in command",
        3 => "invalid options",
        4 => "unimplemented command",
        5 => "command not available",
        200 => "breakpoint could not be set",
        201 => "breakpoint type not supported",
        202 => "invalid breakpoint",
        205 => "no such breakpoint",
        206 => "error evaluating code",
        300 => "can not get property",
        301 => "stack depth invalid",
        302 => "context invalid",
        _ => "debugger error",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineStatus {
    Starting,
    Stopping,
    Stopped,
    Running,
    Break,
}

impl EngineStatus {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "starting" => Self::Starting,
            "stopping" => Self::Stopping,
            "stopped" => Self::Stopped,
            "running" => Self::Running,
            "break" => Self::Break,
            _ => return None,
        })
    }
}

/// Answer to a continuation command (run, step_*), or to `status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusResponse {
    pub status: EngineStatus,
    pub reason: String,
    pub file_uri: Option<String>,
    pub line: Option<u32>,
    pub exception: Option<String>,
    pub message: Option<String>,
}

impl StatusResponse {
    pub fn parse(el: &Element) -> DebugResult<Self> {
        check_error(el)?;
        let status = el
            .attr("status")
            .and_then(EngineStatus::parse)
            .ok_or_else(|| DebugError::Protocol("response without status".into()))?;
        let msg = el.child("message");
        Ok(Self {
            status,
            reason: el.attr("reason").unwrap_or("ok").to_string(),
            file_uri: msg.and_then(|m| m.attr("filename")).map(str::to_string),
            line: msg.and_then(|m| m.attr_u32("lineno")),
            exception: msg.and_then(|m| m.attr("exception")).map(str::to_string),
            message: msg
                .map(|m| m.text.trim().to_string())
                .filter(|t| !t.is_empty()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFrame {
    pub level: u32,
    pub kind: String,
    pub file_uri: String,
    pub line: u32,
    /// Function or method, e.g. `App\Http\Controllers\OrderController->show`.
    pub r#where: String,
}

pub fn parse_stack(el: &Element) -> DebugResult<Vec<RawFrame>> {
    check_error(el)?;
    Ok(el
        .children_named("stack")
        .map(|s| RawFrame {
            level: s.attr_u32("level").unwrap_or(0),
            kind: s.attr("type").unwrap_or("file").to_string(),
            file_uri: s.attr("filename").unwrap_or_default().to_string(),
            line: s.attr_u32("lineno").unwrap_or(0),
            r#where: s.attr("where").unwrap_or("{main}").to_string(),
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextName {
    pub id: u32,
    pub name: String,
}

pub fn parse_context_names(el: &Element) -> DebugResult<Vec<ContextName>> {
    check_error(el)?;
    Ok(el
        .children_named("context")
        .map(|c| ContextName {
            id: c.attr_u32("id").unwrap_or(0),
            name: c.attr("name").unwrap_or_default().to_string(),
        })
        .collect())
}

/// A variable or evaluation result, with already decoded names and values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    pub name: String,
    pub fullname: String,
    /// DBGp type: `int`, `float`, `bool`, `string`, `null`, `array`, `object`, `resource`,
    /// `uninitialized`.
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classname: Option<String>,
    /// Visibility and modifiers, e.g. `public`, `private static`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Length of a string value (the value itself may be truncated by `max_data`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
    pub has_children: bool,
    pub num_children: u32,
    /// Page of `children` (when the engine pages them).
    pub page: u32,
    pub page_size: u32,
    pub children: Vec<Property>,
    /// DAP `variablesReference`: children are fetched with it (0/absent: none or DBGp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<i64>,
    /// DAP: reference of the scope or variable that holds this one (for Set Value and
    /// data breakpoints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<i64>,
    /// DAP: memory location of the value (View Memory).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_reference: Option<String>,
}

fn decoded_text(el: &Element) -> String {
    if el.attr("encoding") == Some("base64") {
        b64_decode(&el.text)
    } else {
        el.text.clone()
    }
}

impl Property {
    pub fn parse(el: &Element) -> Self {
        let name = el
            .child("name")
            .map(decoded_text)
            .or_else(|| el.attr("name").map(str::to_string))
            .unwrap_or_default();
        let fullname = el
            .child("fullname")
            .map(decoded_text)
            .or_else(|| el.attr("fullname").map(str::to_string))
            .unwrap_or_else(|| name.clone());
        let classname = el
            .child("classname")
            .map(decoded_text)
            .or_else(|| el.attr("classname").map(str::to_string))
            .filter(|c| !c.is_empty());
        let r#type = el.attr("type").unwrap_or("undefined").to_string();
        let has_children = el.attr("children") == Some("1");
        let value = if has_children {
            None
        } else if let Some(v) = el.child("value") {
            Some(decoded_text(v))
        } else if r#type == "null" || r#type == "uninitialized" {
            None
        } else {
            Some(decoded_text(el))
        };
        Self {
            name,
            fullname,
            r#type,
            classname,
            facet: el
                .attr("facet")
                .map(str::to_string)
                .filter(|f| !f.is_empty()),
            value,
            size: el.attr_u32("size"),
            has_children,
            num_children: el.attr_u32("numchildren").unwrap_or(0),
            page: el.attr_u32("page").unwrap_or(0),
            page_size: el.attr_u32("pagesize").unwrap_or(0),
            children: el.children_named("property").map(Property::parse).collect(),
            ..Default::default()
        }
    }
}

/// Properties of a `context_get` answer.
pub fn parse_properties(el: &Element) -> DebugResult<Vec<Property>> {
    check_error(el)?;
    Ok(el.children_named("property").map(Property::parse).collect())
}

/// The single property of a `property_get` or `eval` answer.
pub fn parse_property(el: &Element) -> DebugResult<Property> {
    check_error(el)?;
    el.child("property")
        .map(Property::parse)
        .ok_or_else(|| DebugError::Protocol("response without property".into()))
}

/// A breakpoint as the engine reports it (`breakpoint_list`, `breakpoint_get`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBreakpoint {
    pub id: String,
    pub r#type: String,
    pub state: String,
    pub file_uri: Option<String>,
    pub line: Option<u32>,
    pub exception: Option<String>,
    pub hit_count: u32,
    pub resolved: bool,
}

pub fn parse_breakpoints(el: &Element) -> DebugResult<Vec<RemoteBreakpoint>> {
    check_error(el)?;
    Ok(el
        .children_named("breakpoint")
        .map(|b| RemoteBreakpoint {
            id: b.attr("id").unwrap_or_default().to_string(),
            r#type: b.attr("type").unwrap_or("line").to_string(),
            state: b.attr("state").unwrap_or("enabled").to_string(),
            file_uri: b.attr("filename").map(str::to_string),
            line: b.attr_u32("lineno"),
            exception: b.attr("exception").map(str::to_string),
            hit_count: b.attr_u32("hit_count").unwrap_or(0),
            resolved: b.attr("resolved") == Some("resolved"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    #[test]
    fn commands_round_trip() {
        let bytes = encode_command(
            "breakpoint_set",
            12,
            &[
                ('t', "conditional".into()),
                ('f', "file:///var/www/my app/x.php".into()),
                ('n', "7".into()),
            ],
            Some("$a > 1"),
        );
        assert_eq!(bytes.last(), Some(&0));
        let line = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
        assert!(line.contains("-f \"file:///var/www/my app/x.php\""));
        let cmd = parse_command(line).unwrap();
        assert_eq!(cmd.name, "breakpoint_set");
        assert_eq!(cmd.tid, 12);
        assert_eq!(cmd.arg('f'), Some("file:///var/www/my app/x.php"));
        assert_eq!(cmd.arg('n'), Some("7"));
        assert_eq!(cmd.data.as_deref(), Some("$a > 1"));
    }

    #[tokio::test]
    async fn reads_framed_packets() {
        let mut data = frame_packet("<a/>");
        data.extend(frame_packet("<b>ü</b>"));
        let mut r = tokio::io::BufReader::new(&data[..]);
        assert_eq!(read_packet(&mut r).await.unwrap().as_deref(), Some("<a/>"));
        assert_eq!(
            read_packet(&mut r).await.unwrap().as_deref(),
            Some("<b>ü</b>")
        );
        assert_eq!(read_packet(&mut r).await.unwrap(), None);
    }

    #[tokio::test]
    async fn rejects_oversized_packets_without_allocating() {
        let data = b"1000000000000\0<a/>\0".to_vec();
        let mut r = tokio::io::BufReader::new(&data[..]);
        assert!(matches!(
            read_packet(&mut r).await,
            Err(DebugError::Protocol(_))
        ));
        // A header without a NUL is not buffered without bound.
        let data = vec![b'9'; 10_000];
        let mut r = tokio::io::BufReader::new(&data[..]);
        assert!(matches!(
            read_packet(&mut r).await,
            Err(DebugError::Protocol(_))
        ));
        // A truncated body is an error, not a short packet.
        let data = b"10\0<a/>".to_vec();
        let mut r = tokio::io::BufReader::new(&data[..]);
        assert!(read_packet(&mut r).await.is_err());
    }

    #[test]
    fn parses_init() {
        let el = parse(r#"<init xmlns="urn:debugger_protocol_v1" fileuri="file:///var/www/public/index.php" language="PHP" xdebug:language_version="8.3.4" protocol_version="1.0" appid="42" idekey="PHPSTORM"><engine version="3.3.2"><![CDATA[Xdebug]]></engine></init>"#).unwrap();
        let init = InitPacket::parse(&el).unwrap();
        assert_eq!(init.file_uri, "file:///var/www/public/index.php");
        assert_eq!(init.ide_key, "PHPSTORM");
        assert_eq!(init.engine_name, "Xdebug");
        assert_eq!(init.engine_version, "3.3.2");
    }

    #[test]
    fn parses_break_status_and_errors() {
        let el = parse(r#"<response command="step_into" transaction_id="3" status="break" reason="ok"><xdebug:message filename="file:///var/www/a.php" lineno="12"></xdebug:message></response>"#).unwrap();
        let s = StatusResponse::parse(&el).unwrap();
        assert_eq!(s.status, EngineStatus::Break);
        assert_eq!(s.line, Some(12));
        let err = parse(r#"<response command="eval" transaction_id="4"><error code="206"><message><![CDATA[error evaluating code: Undefined constant "foo"]]></message></error></response>"#).unwrap();
        match parse_property(&err) {
            Err(DebugError::Engine { code, message }) => {
                assert_eq!(code, 206);
                assert!(message.contains("Undefined constant"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_nested_paged_and_base64_properties() {
        let el = parse(r#"<response command="property_get" transaction_id="9"><property name="$order" fullname="$order" type="object" classname="App\Models\Order" children="1" numchildren="3" page="0" pagesize="32"><property name="id" fullname="$order-&gt;id" facet="protected" type="int"><![CDATA[42]]></property><property name="note" fullname="$order-&gt;note" type="string" size="5" encoding="base64"><![CDATA[aMOkbGxv]]></property><property type="string" children="0" size="3"><name encoding="base64"><![CDATA[a2V5]]></name><fullname encoding="base64"><![CDATA[JGFbImtleSJd]]></fullname><value encoding="base64"><![CDATA[Zm9v]]></value></property></property></response>"#).unwrap();
        let p = parse_property(&el).unwrap();
        assert_eq!(p.classname.as_deref(), Some("App\\Models\\Order"));
        assert!(p.has_children);
        assert_eq!(p.num_children, 3);
        assert_eq!(p.page_size, 32);
        assert_eq!(p.children[0].fullname, "$order->id");
        assert_eq!(p.children[0].facet.as_deref(), Some("protected"));
        assert_eq!(p.children[0].value.as_deref(), Some("42"));
        assert_eq!(p.children[1].value.as_deref(), Some("hällo"));
        assert_eq!(p.children[2].name, "key");
        assert_eq!(p.children[2].fullname, "$a[\"key\"]");
        assert_eq!(p.children[2].value.as_deref(), Some("foo"));
    }
}
