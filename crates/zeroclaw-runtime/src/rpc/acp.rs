//! Generic Agent Control Protocol (ACP) adapter.
//!
//! This module decouples the ACP capabilities from IDE-specific surfaces,
//! allowing external agent runtimes to interface with ZeroClaw natively.

use anyhow::Result;
use serde_json::Value;

/// The ACP transport adapter.
///
/// Tool execution is intentionally not exposed here until the adapter can call
/// the production dispatcher with the same policy, approval, receipt, and audit
/// path as an ordinary agent turn.
pub struct AcpAdapter;

impl AcpAdapter {
    pub fn new() -> Self {
        Self
    }

    /// Handle a generic ACP command from an external runtime.
    pub async fn handle_command(&mut self, method: &str, params: Value) -> Result<Value> {
        match method {
            "initialize" => {
                // Return server capabilities and versioning.
                Ok(serde_json::json!({
                    "version": "1.0",
                    "capabilities": {
                        "tools": true,
                        "resources": false
                    }
                }))
            }
            "invoke_tool" => {
                let tool_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing tool name"))?;

                let _tool_args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                // This is a protocol acknowledgement only. Do not report a
                // successful tool execution until the production dispatcher is wired.
                let result_output = format!("Tool invocation acknowledged: {tool_name}");

                Ok(serde_json::json!({
                    "success": false,
                    "output": result_output,
                    "error": "ACP tool execution is not wired",
                }))
            }
            _ => {
                anyhow::bail!("Method {} not supported by ACP Adapter", method);
            }
        }
    }
}

// ── Generic outbound ACP adapter contract (P2.4) ────────────────────────────

use serde::{Deserialize, Serialize};

/// Capabilities an external ACP runtime advertises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpCapabilitySet {
    pub tools: bool,
    pub resources: bool,
    pub streaming: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub max_context_tokens: u32,
}

impl Default for AcpCapabilitySet {
    fn default() -> Self {
        Self {
            tools: false,
            resources: false,
            streaming: false,
            structured_output: false,
            vision: false,
            max_context_tokens: 0,
        }
    }
}

/// The capabilities a route requires from an external runtime.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AcpCapabilityRequirements {
    pub tools: bool,
    pub streaming: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub min_context_tokens: u32,
}

/// Negotiate the runtime's capabilities against what the route requires. Fails
/// closed: an incompatible runtime is rejected, never silently downgraded.
pub fn negotiate(
    remote: &AcpCapabilitySet,
    required: &AcpCapabilityRequirements,
) -> Result<AcpCapabilitySet> {
    let mut missing = Vec::new();
    if required.tools && !remote.tools {
        missing.push("tools");
    }
    if required.streaming && !remote.streaming {
        missing.push("streaming");
    }
    if required.structured_output && !remote.structured_output {
        missing.push("structured_output");
    }
    if required.vision && !remote.vision {
        missing.push("vision");
    }
    if remote.max_context_tokens < required.min_context_tokens {
        missing.push("context_window");
    }
    anyhow::ensure!(
        missing.is_empty(),
        "external ACP runtime is incompatible; missing: {}",
        missing.join(", ")
    );
    Ok(*remote)
}

/// Policy, approvals, cancellation, and attribution propagated to an external
/// runtime so its work stays attributable and bounded by ZeroClaw's ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcpPropagation {
    /// Owning agent alias.
    pub agent: String,
    /// Canonical task id, when the external run is a task.
    pub task_id: Option<String>,
    /// Session key for context continuity.
    pub session_key: Option<String>,
    /// Risk profile the external runtime must honour.
    pub risk_profile: String,
    /// Whether the external runtime must route risky steps through approvals.
    pub approvals_required: bool,
    /// Cancellation token id the runtime must observe.
    pub cancellation_id: String,
}

/// The lifecycle of an outbound ACP session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpSessionState {
    Starting,
    Running,
    Closed,
}

impl AcpSessionState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Starting, Self::Running)
                | (Self::Starting, Self::Closed)
                | (Self::Running, Self::Closed)
        )
    }
}

/// A supervised outbound ACP session: negotiates capabilities, carries the
/// propagation envelope, and enforces the lifecycle state machine.
#[derive(Debug, Clone)]
pub struct AcpOutboundSession {
    pub state: AcpSessionState,
    pub capabilities: AcpCapabilitySet,
    pub propagation: AcpPropagation,
}

impl AcpOutboundSession {
    /// Start a session after negotiating capabilities. Fails closed on an
    /// incompatible runtime.
    pub fn start(
        propagation: AcpPropagation,
        remote: &AcpCapabilitySet,
        required: &AcpCapabilityRequirements,
    ) -> Result<Self> {
        let capabilities = negotiate(remote, required)?;
        Ok(Self {
            state: AcpSessionState::Running,
            capabilities,
            propagation,
        })
    }

    /// Move a `Starting` session to `Running`.
    pub fn mark_running(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.state.can_transition_to(AcpSessionState::Running),
            "cannot move session from {:?} to Running",
            self.state
        );
        self.state = AcpSessionState::Running;
        Ok(())
    }

    /// Close the session (idempotent at `Closed`).
    pub fn close(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.state == AcpSessionState::Closed || self.state.can_transition_to(AcpSessionState::Closed),
            "cannot close session from {:?}",
            self.state
        );
        self.state = AcpSessionState::Closed;
        Ok(())
    }

    /// Recover a session whose process died: reopen to `Starting` only if it was
    /// not cleanly closed. A closed session must not be silently resumed.
    pub fn recover(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.state != AcpSessionState::Closed,
            "refusing to recover a cleanly closed ACP session"
        );
        self.state = AcpSessionState::Starting;
        Ok(())
    }
}

/// Conversation context carried across a runtime switch. Switching runtimes
/// preserves the history verbatim; only the runtime label changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpContext {
    pub runtime: String,
    pub messages: Vec<serde_json::Value>,
}

impl AcpContext {
    pub fn switch_runtime(&mut self, to: impl Into<String>) {
        self.runtime = to.into();
    }
}

// ── Live transport: line-delimited JSON-RPC 2.0 over async IO (P2.4) ────────

use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter,
};

/// Maximum bytes of a single ACP frame (one JSON line).
pub const MAX_ACP_FRAME_BYTES: usize = 1 << 20;

/// Read one newline-terminated frame, bounded to `max` bytes. Returns `None` at
/// clean EOF.
async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    max: usize,
) -> Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(None)
            } else {
                Ok(Some(buf))
            };
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            buf.extend_from_slice(&available[..=pos]);
            reader.consume(pos + 1);
            break;
        }
        let len = available.len();
        buf.extend_from_slice(available);
        reader.consume(len);
        anyhow::ensure!(buf.len() <= max, "ACP frame exceeds {max} bytes");
    }
    anyhow::ensure!(buf.len() <= max, "ACP frame exceeds {max} bytes");
    while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
        buf.pop();
    }
    Ok(Some(buf))
}

/// A line-delimited JSON-RPC 2.0 transport over an async read/write pair.
pub struct AcpTransport<R, W> {
    reader: BufReader<R>,
    writer: BufWriter<W>,
    next_id: u64,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AcpTransport<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
            next_id: 1,
        }
    }

    /// Send a request and await its matching response. Fails closed on a closed
    /// stream, an oversized/malformed frame, an id mismatch, or a JSON-RPC error.
    pub async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;

        // Skip startup banners / non-JSON diagnostics a child may emit before
        // its protocol stream begins (bounded), then match the response id.
        let mut response: Option<serde_json::Value> = None;
        for _ in 0..64 {
            let frame = read_frame(&mut self.reader, MAX_ACP_FRAME_BYTES)
                .await?
                .ok_or_else(|| anyhow::anyhow!("ACP transport closed while awaiting a response"))?;
            match serde_json::from_slice::<serde_json::Value>(&frame) {
                Ok(value) if value.get("id").is_some() || value.get("jsonrpc").is_some() => {
                    response = Some(value);
                    break;
                }
                // Non-protocol line (banner/log) — skip and keep reading.
                _ => continue,
            }
        }
        let response = response
            .ok_or_else(|| anyhow::anyhow!("ACP produced no JSON-RPC response after 64 frames"))?;
        anyhow::ensure!(
            response.get("id").and_then(serde_json::Value::as_u64) == Some(id),
            "ACP response id does not match request {id}"
        );
        if let Some(error) = response.get("error") {
            anyhow::bail!("ACP error response: {error}");
        }
        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

/// Parse an `initialize` result into a capability set, failing closed (`false`/
/// `0`) for any field the runtime omits.
fn parse_capabilities(result: &serde_json::Value) -> Result<AcpCapabilitySet> {
    let capabilities = result
        .get("capabilities")
        .ok_or_else(|| anyhow::anyhow!("ACP initialize response is missing `capabilities`"))?;
    let flag = |key: &str| {
        capabilities
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    };
    Ok(AcpCapabilitySet {
        tools: flag("tools"),
        resources: flag("resources"),
        streaming: flag("streaming"),
        structured_output: flag("structured_output"),
        vision: flag("vision"),
        max_context_tokens: capabilities
            .get("max_context_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
    })
}

/// An outbound ACP client: negotiates capabilities on `initialize`, then routes
/// tool invocations over the transport under the session's propagation.
pub struct AcpClient<R, W> {
    transport: AcpTransport<R, W>,
    session: Option<AcpOutboundSession>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AcpClient<R, W> {
    pub fn new(transport: AcpTransport<R, W>) -> Self {
        Self {
            transport,
            session: None,
        }
    }

    /// Initialize the external runtime, negotiate capabilities, and start the
    /// session. Fails closed on an incompatible runtime.
    pub async fn initialize(
        &mut self,
        propagation: AcpPropagation,
        required: &AcpCapabilityRequirements,
    ) -> Result<&AcpOutboundSession> {
        let result = self.transport.call("initialize", serde_json::json!({})).await?;
        let remote = parse_capabilities(&result)?;
        let session = AcpOutboundSession::start(propagation, &remote, required)?;
        self.session = Some(session);
        Ok(self.session.as_ref().expect("session set above"))
    }

    /// Invoke a tool on the external runtime. Requires a started session.
    pub async fn invoke_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        anyhow::ensure!(
            self.session.is_some(),
            "ACP client must be initialized before invoking a tool"
        );
        self.transport
            .call("invoke_tool", serde_json::json!({ "name": name, "arguments": arguments }))
            .await
    }

    pub fn session(&self) -> Option<&AcpOutboundSession> {
        self.session.as_ref()
    }
}

/// A spawned external ACP runtime process plus its transport.
pub struct AcpProcess {
    child: tokio::process::Child,
}

impl AcpProcess {
    /// Spawn an external runtime and connect a transport to its stdio.
    pub fn spawn(
        program: &str,
        args: &[String],
    ) -> Result<(
        Self,
        AcpTransport<tokio::process::ChildStdout, tokio::process::ChildStdin>,
    )> {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("ACP child has no stdout"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("ACP child has no stdin"))?;
        Ok((Self { child }, AcpTransport::new(stdout, stdin)))
    }

    /// Terminate and reap the child.
    pub async fn close(mut self) -> Result<()> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> AcpCapabilitySet {
        AcpCapabilitySet {
            tools: true,
            resources: true,
            streaming: true,
            structured_output: true,
            vision: true,
            max_context_tokens: 200_000,
        }
    }

    fn propagation() -> AcpPropagation {
        AcpPropagation {
            agent: "main".into(),
            task_id: Some("task-1".into()),
            session_key: Some("sess-1".into()),
            risk_profile: "supervised".into(),
            approvals_required: true,
            cancellation_id: "cancel-1".into(),
        }
    }

    #[test]
    fn negotiation_rejects_incompatible_runtime() {
        let required = AcpCapabilityRequirements {
            tools: true,
            vision: true,
            min_context_tokens: 100_000,
            ..Default::default()
        };
        assert!(negotiate(&full(), &required).is_ok());

        let no_vision = AcpCapabilitySet {
            vision: false,
            ..full()
        };
        let err = negotiate(&no_vision, &required).unwrap_err().to_string();
        assert!(err.contains("vision"));

        let small = AcpCapabilitySet {
            max_context_tokens: 1000,
            ..full()
        };
        assert!(negotiate(&small, &required).is_err());
    }

    #[test]
    fn session_start_carries_propagation_and_enforces_lifecycle() {
        let mut session =
            AcpOutboundSession::start(propagation(), &full(), &AcpCapabilityRequirements::default())
                .unwrap();
        assert_eq!(session.state, AcpSessionState::Running);
        // Propagation is carried verbatim.
        assert_eq!(session.propagation, propagation());
        assert!(session.propagation.approvals_required);
        assert_eq!(session.propagation.cancellation_id, "cancel-1");
        assert_eq!(session.propagation.task_id.as_deref(), Some("task-1"));

        session.close().unwrap();
        assert_eq!(session.state, AcpSessionState::Closed);
        session.close().unwrap(); // idempotent
        assert!(session.mark_running().is_err());
    }

    #[test]
    fn recovery_rejects_a_cleanly_closed_session() {
        let mut session =
            AcpOutboundSession::start(propagation(), &full(), &AcpCapabilityRequirements::default())
                .unwrap();
        session.recover().unwrap();
        assert_eq!(session.state, AcpSessionState::Starting);
        session.mark_running().unwrap();
        session.close().unwrap();
        assert!(session.recover().is_err(), "closed session must not resume");
    }

    #[test]
    fn runtime_switch_preserves_context() {
        let mut ctx = AcpContext {
            runtime: "acp:kiro".into(),
            messages: vec![serde_json::json!({"role": "user", "content": "hi"})],
        };
        let before = ctx.messages.clone();
        ctx.switch_runtime("acp:other");
        assert_eq!(ctx.runtime, "acp:other");
        assert_eq!(ctx.messages, before, "history must be preserved verbatim");
    }

    async fn fake_server(io: tokio::io::DuplexStream) {
        let (r, mut w) = tokio::io::split(io);
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            let result = match request["method"].as_str().unwrap_or("") {
                "initialize" => serde_json::json!({
                    "version": "1.0",
                    "capabilities": {
                        "tools": true,
                        "streaming": true,
                        "structured_output": true,
                        "vision": true,
                        "max_context_tokens": 100_000
                    }
                }),
                "invoke_tool" => serde_json::json!({ "success": true }),
                _ => serde_json::json!(null),
            };
            let response =
                serde_json::json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
            let mut encoded = serde_json::to_string(&response).unwrap();
            encoded.push('\n');
            let _ = w.write_all(encoded.as_bytes()).await;
            let _ = w.flush().await;
        }
    }

    #[tokio::test]
    async fn transport_round_trips_and_negotiates() {
        let (client_io, server_io) = tokio::io::duplex(8192);
        let server = tokio::spawn(fake_server(server_io));
        let (r, w) = tokio::io::split(client_io);
        let mut client = AcpClient::new(AcpTransport::new(r, w));

        let required = AcpCapabilityRequirements {
            tools: true,
            vision: true,
            min_context_tokens: 1000,
            ..Default::default()
        };
        let session = client.initialize(propagation(), &required).await.unwrap();
        assert!(session.capabilities.tools);
        assert!(session.capabilities.vision);
        assert_eq!(session.propagation.agent, "main");

        let out = client
            .invoke_tool("shell", serde_json::json!({"command": "date"}))
            .await
            .unwrap();
        assert_eq!(out["success"], serde_json::json!(true));

        drop(client);
        let _ = server.await;
    }

    #[tokio::test]
    async fn read_frame_rejects_oversized_and_reports_clean_eof() {
        let (a, mut b) = tokio::io::duplex(64);
        b.write_all(b"1234567890").await.unwrap();
        drop(b);
        let mut reader = BufReader::new(a);
        assert!(read_frame(&mut reader, 8).await.is_err(), "oversized frame");

        let (a2, b2) = tokio::io::duplex(64);
        drop(b2);
        let mut reader2 = BufReader::new(a2);
        assert!(read_frame(&mut reader2, 8).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn transport_errors_on_closed_stream() {
        let (r, w) = tokio::io::duplex(64);
        let (cr, cw) = tokio::io::split(r);
        drop(w); // close the peer's write end so no response can arrive
        let mut transport = AcpTransport::new(cr, cw);
        assert!(
            transport
                .call("initialize", serde_json::json!({}))
                .await
                .is_err()
        );
    }

    /// Child-process ACP echo server. Spawned by
    /// `acp_process_round_trips_against_a_real_child`; never run directly.
    #[test]
    #[ignore = "spawned as a child process by acp_process_round_trips_against_a_real_child"]
    fn acp_echo_helper() {
        use std::io::{BufRead, Write};
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let id = request
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let result = match request.get("method").and_then(|m| m.as_str()).unwrap_or("") {
                "initialize" => serde_json::json!({
                    "capabilities": {"tools": true, "vision": true, "max_context_tokens": 100_000}
                }),
                _ => serde_json::json!({"ok": true}),
            };
            let response = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result});
            let mut out = stdout.lock();
            let _ = writeln!(out, "{response}");
            let _ = out.flush();
        }
    }

    #[tokio::test]
    async fn acp_process_round_trips_against_a_real_child() {
        let exe = std::env::current_exe().expect("current test exe");
        let (process, mut transport) = AcpProcess::spawn(
            exe.to_str().expect("exe path is UTF-8"),
            &[
                "--ignored".to_string(),
                "--exact".to_string(),
                "rpc::acp::tests::acp_echo_helper".to_string(),
            ],
        )
        .expect("spawn ACP helper child");

        let caps = transport
            .call("initialize", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(caps["capabilities"]["tools"], serde_json::json!(true));
        let invoked = transport
            .call("invoke_tool", serde_json::json!({"name": "shell"}))
            .await
            .unwrap();
        assert_eq!(invoked["ok"], serde_json::json!(true));

        process.close().await.unwrap();
    }
}
