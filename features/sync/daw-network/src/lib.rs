// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! TCP peer mesh — connects sync engine instances over the LAN.
//!
//! [`PeerMesh`] binds a TCP listener, accepts inbound connections, and connects
//! outbound to discovered peers. SyncEvents are serialized with facet-postcard
//! and framed as `[4-byte LE length][payload]`.
//!
//! Handshake protocol: on connect, both sides exchange a [`Handshake`] frame
//! containing their `peer_id` and `session_id`. Connections with mismatched
//! session IDs are dropped. Duplicate connections are resolved by tiebreak:
//! the peer with the lexicographically lower `peer_id` keeps its outbound
//! connection, the other side drops theirs.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use daw_synchronization::SyncEvent;
use facet::Facet;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};

use daw_synchronization::suppression::SuppressionSet;

/// Configuration for the peer mesh.
#[derive(Clone, Debug)]
pub struct MeshConfig {
    /// This peer's unique identifier.
    pub peer_id: String,
    /// Session identifier — only peers in the same session connect.
    pub session_id: String,
}

/// Handshake message exchanged on connection.
#[derive(Clone, Debug, Facet)]
pub struct Handshake {
    pub peer_id: String,
    pub session_id: String,
}

// ── Clock calibration frames ────────────────────────────────────────────────

/// Number of clock probes during calibration.
const CLOCK_PROBE_COUNT: u8 = 5;

/// Clock probe sent by the outbound peer to measure clock offset.
#[derive(Clone, Debug, Facet)]
struct ClockProbe {
    probe_index: u8,
    /// Prober's wall-clock time when this probe was sent (ms since epoch).
    send_time_ms: u64,
}

/// Clock probe response sent back by the inbound peer.
#[derive(Clone, Debug, Facet)]
struct ClockProbeResponse {
    probe_index: u8,
    /// Echoed send_time_ms from the probe.
    prober_send_time_ms: u64,
    /// Responder's wall-clock time when it received the probe (ms since epoch).
    responder_time_ms: u64,
}

/// Final calibration result sent by the prober after computing offset.
#[derive(Clone, Debug, Facet)]
struct CalibrationResult {
    /// Estimated clock offset in milliseconds.
    /// Positive means remote clock is ahead of prober's clock.
    offset_ms: i64,
}

/// Peer lifecycle event emitted by the mesh.
#[derive(Clone, Debug)]
pub enum MeshPeerEvent {
    /// A new peer completed the handshake and is connected.
    Connected(String),
    /// A peer disconnected.
    Disconnected(String),
}

/// A connected peer's write channel and cancellation token.
struct PeerConnection {
    write_tx: mpsc::Sender<Vec<u8>>,
    cancel: CancellationToken,
    /// Estimated clock offset for this peer in milliseconds.
    /// `offset = remote_clock - local_clock`. Subtract from received
    /// `created_at_ms` to translate remote timestamps to local time.
    clock_offset_ms: i64,
}

/// TCP peer mesh for forwarding SyncEvents between instances.
pub struct PeerMesh {
    config: MeshConfig,
    listener_port: u16,
    peers: Arc<tokio::sync::Mutex<HashMap<String, PeerConnection>>>,
    incoming_tx: mpsc::Sender<SyncEvent>,
    peer_event_tx: mpsc::Sender<MeshPeerEvent>,
    cancel: CancellationToken,
}

impl PeerMesh {
    /// Bind a TCP listener and start forwarding events.
    ///
    /// Returns `(mesh, incoming_rx)` where `incoming_rx` yields SyncEvents
    /// received from remote peers.
    pub async fn bind(
        config: MeshConfig,
        event_rx: broadcast::Receiver<SyncEvent>,
        suppression: Arc<tokio::sync::Mutex<SuppressionSet>>,
    ) -> io::Result<(
        Self,
        mpsc::Receiver<SyncEvent>,
        mpsc::Receiver<MeshPeerEvent>,
    )> {
        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let port = listener.local_addr()?.port();
        info!(port, peer_id = %config.peer_id, "PeerMesh bound");

        let (incoming_tx, incoming_rx) = mpsc::channel(4096);
        let (peer_event_tx, peer_event_rx) = mpsc::channel(64);
        let peers: Arc<tokio::sync::Mutex<HashMap<String, PeerConnection>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let cancel = CancellationToken::new();

        let mesh = Self {
            config: config.clone(),
            listener_port: port,
            peers: Arc::clone(&peers),
            incoming_tx: incoming_tx.clone(),
            peer_event_tx: peer_event_tx.clone(),
            cancel: cancel.clone(),
        };

        // Spawn accept loop
        let accept_config = config.clone();
        let accept_peers = Arc::clone(&peers);
        let accept_tx = incoming_tx.clone();
        let accept_cancel = cancel.clone();
        let accept_peer_tx = peer_event_tx.clone();
        tokio::task::spawn(async move {
            accept_loop(
                listener,
                accept_config,
                accept_peers,
                accept_tx,
                accept_peer_tx,
                accept_cancel,
            )
            .await;
        });

        // Spawn broadcast forwarder — reads from engine broadcast and sends to all peers
        let fwd_peers = Arc::clone(&peers);
        let fwd_cancel = cancel.clone();
        tokio::task::spawn(async move {
            broadcast_forwarder(event_rx, fwd_peers, fwd_cancel, suppression).await;
        });

        Ok((mesh, incoming_rx, peer_event_rx))
    }

    /// The TCP port this mesh is listening on.
    pub fn local_port(&self) -> u16 {
        self.listener_port
    }

    /// Initiate an outbound connection to a discovered peer.
    pub async fn connect_peer(&self, addr: SocketAddr, remote_peer_id: String) {
        // Dedup: only the lexicographically lower peer_id initiates
        if self.config.peer_id >= remote_peer_id {
            debug!(
                remote = %remote_peer_id,
                "Skipping outbound — remote should initiate (tiebreak)"
            );
            return;
        }

        // Already connected?
        if self.peers.lock().await.contains_key(&remote_peer_id) {
            debug!(remote = %remote_peer_id, "Already connected");
            return;
        }

        let config = self.config.clone();
        let peers = Arc::clone(&self.peers);
        let incoming_tx = self.incoming_tx.clone();
        let peer_event_tx = self.peer_event_tx.clone();
        let cancel = self.cancel.clone();

        tokio::task::spawn(async move {
            match TcpStream::connect(addr).await {
                Ok(stream) => {
                    info!(addr = %addr, remote = %remote_peer_id, "Outbound connection established");
                    if let Err(e) = handle_connection(
                        stream,
                        config,
                        peers,
                        incoming_tx,
                        peer_event_tx,
                        cancel,
                        true,
                    )
                    .await
                    {
                        warn!(addr = %addr, "Outbound connection failed: {e}");
                    }
                }
                Err(e) => {
                    warn!(addr = %addr, remote = %remote_peer_id, "Failed to connect: {e}");
                }
            }
        });
    }

    /// Remove a disconnected peer.
    pub async fn remove_peer(&self, peer_id: &str) {
        if let Some(conn) = self.peers.lock().await.remove(peer_id) {
            conn.cancel.cancel();
            info!(peer_id, "Peer removed");
        }
    }

    /// Number of currently connected peers.
    pub async fn peer_count(&self) -> usize {
        self.peers.lock().await.len()
    }
}

impl Drop for PeerMesh {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

// ── Wire format helpers ─────────────────────────────────────────────────────

fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

async fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

async fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    let frame = encode_frame(payload);
    stream.write_all(&frame).await?;
    Ok(())
}

// ── Accept loop ─────────────────────────────────────────────────────────────

async fn accept_loop(
    listener: TcpListener,
    config: MeshConfig,
    peers: Arc<tokio::sync::Mutex<HashMap<String, PeerConnection>>>,
    incoming_tx: mpsc::Sender<SyncEvent>,
    peer_event_tx: mpsc::Sender<MeshPeerEvent>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        debug!(addr = %addr, "Inbound connection");
                        let config = config.clone();
                        let peers = Arc::clone(&peers);
                        let tx = incoming_tx.clone();
                        let ptx = peer_event_tx.clone();
                        let cancel = cancel.clone();
                        tokio::task::spawn(async move {
                            if let Err(e) = handle_connection(stream, config, peers, tx, ptx, cancel, false).await {
                                debug!(addr = %addr, "Inbound connection ended: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        warn!("Accept error: {e}");
                    }
                }
            }
        }
    }
}

// ── Clock calibration ────────────────────────────────────────────────────────

/// Run clock calibration as the prober (outbound side).
/// Returns estimated offset: `remote_clock - local_clock` in ms.
async fn calibrate_as_prober(stream: &mut TcpStream) -> io::Result<i64> {
    let mut offsets = Vec::with_capacity(CLOCK_PROBE_COUNT as usize);

    for i in 0..CLOCK_PROBE_COUNT {
        let send_time = SyncEvent::now_ms();
        let probe = ClockProbe {
            probe_index: i,
            send_time_ms: send_time,
        };
        let bytes = facet_postcard::to_vec(&probe).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("probe serialize: {e}"))
        })?;
        write_frame(stream, &bytes).await?;

        let resp_bytes = read_frame(stream).await?;
        let recv_time = SyncEvent::now_ms();
        let resp: ClockProbeResponse = facet_postcard::from_slice(&resp_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("probe response deserialize: {e}"),
            )
        })?;

        let rtt = recv_time.saturating_sub(send_time);
        // NTP offset formula: offset = remote_time - (send_time + RTT/2)
        let offset = resp.responder_time_ms as i64 - (send_time as i64 + rtt as i64 / 2);
        offsets.push(offset);
    }

    // Median for robustness against outlier probes
    offsets.sort();
    let median_offset = offsets[offsets.len() / 2];

    // Send result to responder so it can compute its own (negated) offset
    let result = CalibrationResult {
        offset_ms: median_offset,
    };
    let result_bytes = facet_postcard::to_vec(&result).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("calibration result serialize: {e}"),
        )
    })?;
    write_frame(stream, &result_bytes).await?;

    Ok(median_offset)
}

/// Run clock calibration as the responder (inbound side).
/// Returns the negated offset (from the responder's perspective).
async fn calibrate_as_responder(stream: &mut TcpStream) -> io::Result<i64> {
    for _ in 0..CLOCK_PROBE_COUNT {
        let probe_bytes = read_frame(stream).await?;
        let now = SyncEvent::now_ms();
        let probe: ClockProbe = facet_postcard::from_slice(&probe_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("probe deserialize: {e}"),
            )
        })?;

        let resp = ClockProbeResponse {
            probe_index: probe.probe_index,
            prober_send_time_ms: probe.send_time_ms,
            responder_time_ms: now,
        };
        let resp_bytes = facet_postcard::to_vec(&resp).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("probe response serialize: {e}"),
            )
        })?;
        write_frame(stream, &resp_bytes).await?;
    }

    // Read calibration result from prober
    let result_bytes = read_frame(stream).await?;
    let result: CalibrationResult = facet_postcard::from_slice(&result_bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("calibration result deserialize: {e}"),
        )
    })?;

    // Negate: if prober says we're +50ms ahead, then from our perspective
    // the prober is -50ms (behind us)
    Ok(-result.offset_ms)
}

// ── Connection handler (used for both inbound and outbound) ─────────────────

async fn handle_connection(
    mut stream: TcpStream,
    config: MeshConfig,
    peers: Arc<tokio::sync::Mutex<HashMap<String, PeerConnection>>>,
    incoming_tx: mpsc::Sender<SyncEvent>,
    peer_event_tx: mpsc::Sender<MeshPeerEvent>,
    mesh_cancel: CancellationToken,
    is_outbound: bool,
) -> io::Result<()> {
    // Send our handshake
    let hs = Handshake {
        peer_id: config.peer_id.clone(),
        session_id: config.session_id.clone(),
    };
    let hs_bytes = facet_postcard::to_vec(&hs).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("handshake serialize: {e}"),
        )
    })?;
    write_frame(&mut stream, &hs_bytes).await?;

    // Read remote handshake
    let remote_bytes = read_frame(&mut stream).await?;
    let remote_hs: Handshake = facet_postcard::from_slice(&remote_bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("handshake deserialize: {e}"),
        )
    })?;

    // Validate session
    if remote_hs.session_id != config.session_id {
        warn!(
            remote_peer = %remote_hs.peer_id,
            remote_session = %remote_hs.session_id,
            local_session = %config.session_id,
            "Session mismatch — dropping connection"
        );
        return Ok(());
    }

    let remote_peer_id = remote_hs.peer_id.clone();

    // Check for existing connection
    {
        let peers_lock = peers.lock().await;
        if peers_lock.contains_key(&remote_peer_id) {
            debug!(remote = %remote_peer_id, "Duplicate connection — dropping");
            return Ok(());
        }
    }

    info!(remote = %remote_peer_id, is_outbound, "Peer connected");

    // Clock calibration — measure clock offset before splitting the stream
    let clock_offset_ms = if is_outbound {
        calibrate_as_prober(&mut stream).await?
    } else {
        calibrate_as_responder(&mut stream).await?
    };
    info!(
        remote = %remote_peer_id,
        clock_offset_ms,
        "Clock calibration complete"
    );

    // Split the stream
    let (read_half, mut write_half) = stream.into_split();

    // Create write channel
    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(256);
    let peer_cancel = CancellationToken::new();

    // Register the peer
    {
        let mut peers_lock = peers.lock().await;
        peers_lock.insert(
            remote_peer_id.clone(),
            PeerConnection {
                write_tx,
                cancel: peer_cancel.clone(),
                clock_offset_ms,
            },
        );
    }
    let _ = peer_event_tx
        .send(MeshPeerEvent::Connected(remote_peer_id.clone()))
        .await;

    // Spawn writer task
    let writer_cancel = peer_cancel.clone();
    let writer_peer_id = remote_peer_id.clone();
    tokio::task::spawn(async move {
        loop {
            tokio::select! {
                _ = writer_cancel.cancelled() => break,
                msg = write_rx.recv() => {
                    match msg {
                        Some(data) => {
                            let frame = encode_frame(&data);
                            if let Err(e) = write_half.write_all(&frame).await {
                                debug!(peer = %writer_peer_id, "Write error: {e}");
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Reader loop (runs on current task)
    let mut read_half = tokio::io::BufReader::new(read_half);

    // Simulated network latency for testing cross-machine sync on a single host.
    // Set FTS_SYNC_SIMULATED_LATENCY_MS=5 to add 5ms one-way delay to all
    // incoming messages (simulates ~10ms RTT LAN conditions).
    let simulated_latency = std::env::var("FTS_SYNC_SIMULATED_LATENCY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(std::time::Duration::from_millis);
    let simulated_jitter = std::env::var("FTS_SYNC_SIMULATED_JITTER_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());

    let reader_result: io::Result<()> = async {
        loop {
            tokio::select! {
                _ = mesh_cancel.cancelled() => break,
                _ = peer_cancel.cancelled() => break,
                result = read_frame_from_buf(&mut read_half) => {
                    match result {
                        Ok(payload) => {
                            match facet_postcard::from_slice::<SyncEvent>(&payload) {
                                Ok(mut event) => {
                                    // Adjust timestamp from remote clock to local clock.
                                    // offset = remote - local, so local_time = remote_time - offset
                                    if clock_offset_ms != 0 {
                                        event.created_at_ms = (event.created_at_ms as i64
                                            - clock_offset_ms)
                                            as u64;
                                    }

                                    // Inject simulated network delay for testing.
                                    if let Some(base_delay) = simulated_latency {
                                        let jitter = simulated_jitter
                                            .map(|j| {
                                                use std::hash::{Hash, Hasher};
                                                let mut h = std::hash::DefaultHasher::new();
                                                event.sequence.hash(&mut h);
                                                std::time::Duration::from_millis(h.finish() % (j + 1))
                                            })
                                            .unwrap_or_default();
                                        tokio::time::sleep(base_delay + jitter).await;
                                    }

                                    if incoming_tx.send(event).await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!(peer = %remote_peer_id, "Deserialize error: {e}");
                                }
                            }
                        }
                        Err(_) => break, // Connection closed or error
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    // Cleanup
    peer_cancel.cancel();
    peers.lock().await.remove(&remote_peer_id);
    let _ = peer_event_tx
        .send(MeshPeerEvent::Disconnected(remote_peer_id.clone()))
        .await;
    info!(remote = %remote_peer_id, "Peer disconnected");

    reader_result
}

/// Read a length-prefixed frame from a buffered reader.
async fn read_frame_from_buf<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut tokio::io::BufReader<R>,
) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len > 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }

    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

// ── Broadcast forwarder ─────────────────────────────────────────────────────

async fn broadcast_forwarder(
    mut event_rx: broadcast::Receiver<SyncEvent>,
    peers: Arc<tokio::sync::Mutex<HashMap<String, PeerConnection>>>,
    cancel: CancellationToken,
    suppression: Arc<tokio::sync::Mutex<SuppressionSet>>,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        // Check if this event was recently applied from a remote peer
                        // (echo suppression — prevents infinite feedback loops)
                        {
                            let sup = suppression.lock().await;
                            if daw_synchronization::engine::is_event_suppressed(&sup, &event) {
                                trace!(
                                    "Suppressed outbound echo for {:?}",
                                    std::mem::discriminant(&event.domain),
                                );
                                continue;
                            }
                        }

                        let payload = match facet_postcard::to_vec(&event) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!("Failed to serialize SyncEvent: {e}");
                                continue;
                            }
                        };

                        let peers_lock = peers.lock().await;
                        for (peer_id, conn) in peers_lock.iter() {
                            // Don't send events back to their origin
                            if peer_id == &event.origin_peer {
                                continue;
                            }
                            if conn.write_tx.try_send(payload.clone()).is_err() {
                                debug!(peer = %peer_id, "Write channel full or closed");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Broadcast forwarder lagged by {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_synchronization::SyncDomain;
    use tokio::sync::broadcast;

    fn empty_suppression() -> Arc<tokio::sync::Mutex<SuppressionSet>> {
        Arc::new(tokio::sync::Mutex::new(SuppressionSet::new()))
    }

    fn make_event(origin: &str, seq: u64) -> SyncEvent {
        use daw::service::Transport;
        SyncEvent {
            origin_peer: origin.to_string(),
            sequence: seq,
            project_guid: "proj-1".to_string(),
            domain: SyncDomain::Transport(Transport::default()),
            created_at_ms: SyncEvent::now_ms(),
        }
    }

    #[tokio::test]
    async fn two_meshes_exchange_events() {
        let (tx_a, _) = broadcast::channel(64);
        let (tx_b, _) = broadcast::channel(64);

        let config_a = MeshConfig {
            peer_id: "alpha".to_string(),
            session_id: "test-session".to_string(),
        };
        let config_b = MeshConfig {
            peer_id: "beta".to_string(),
            session_id: "test-session".to_string(),
        };

        let (mesh_a, mut rx_a, _pe_a) =
            PeerMesh::bind(config_a, tx_a.subscribe(), empty_suppression())
                .await
                .unwrap();
        let (mesh_b, mut rx_b, _pe_b) =
            PeerMesh::bind(config_b, tx_b.subscribe(), empty_suppression())
                .await
                .unwrap();

        // Connect alpha → beta (alpha < beta, so alpha initiates)
        let addr_b = SocketAddr::from(([127, 0, 0, 1], mesh_b.local_port()));
        mesh_a.connect_peer(addr_b, "beta".to_string()).await;

        // Wait for connection to establish
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(mesh_a.peer_count().await, 1);
        assert_eq!(mesh_b.peer_count().await, 1);

        // Send event from A's broadcast → should arrive at B's incoming
        let event = make_event("alpha", 1);
        tx_a.send(event.clone()).unwrap();

        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx_b.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");

        assert_eq!(received.origin_peer, "alpha");
        assert_eq!(received.sequence, 1);

        // Send event from B's broadcast → should arrive at A's incoming
        let event_b = make_event("beta", 1);
        tx_b.send(event_b).unwrap();

        let received_a = tokio::time::timeout(std::time::Duration::from_secs(2), rx_a.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");

        assert_eq!(received_a.origin_peer, "beta");
    }

    #[tokio::test]
    async fn three_meshes_form_full_mesh() {
        let (tx_a, _) = broadcast::channel(64);
        let (tx_b, _) = broadcast::channel(64);
        let (tx_c, _) = broadcast::channel(64);

        let (mesh_a, mut _rx_a, _pe_a) = PeerMesh::bind(
            MeshConfig {
                peer_id: "a".to_string(),
                session_id: "s".to_string(),
            },
            tx_a.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let (mesh_b, mut rx_b, _pe_b) = PeerMesh::bind(
            MeshConfig {
                peer_id: "b".to_string(),
                session_id: "s".to_string(),
            },
            tx_b.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let (mesh_c, mut rx_c, _pe_c) = PeerMesh::bind(
            MeshConfig {
                peer_id: "c".to_string(),
                session_id: "s".to_string(),
            },
            tx_c.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let addr_b = SocketAddr::from(([127, 0, 0, 1], mesh_b.local_port()));
        let addr_c = SocketAddr::from(([127, 0, 0, 1], mesh_c.local_port()));

        // a < b, a < c, b < c — so "a" initiates to "b" and "c", "b" initiates to "c"
        mesh_a.connect_peer(addr_b, "b".to_string()).await;
        mesh_a.connect_peer(addr_c, "c".to_string()).await;
        mesh_b.connect_peer(addr_c, "c".to_string()).await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert_eq!(mesh_a.peer_count().await, 2);
        assert_eq!(mesh_b.peer_count().await, 2);
        assert_eq!(mesh_c.peer_count().await, 2);

        // Event from A should reach B and C
        tx_a.send(make_event("a", 1)).unwrap();

        let b_got = tokio::time::timeout(std::time::Duration::from_secs(2), rx_b.recv())
            .await
            .unwrap()
            .unwrap();
        let c_got = tokio::time::timeout(std::time::Duration::from_secs(2), rx_c.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(b_got.origin_peer, "a");
        assert_eq!(c_got.origin_peer, "a");
    }

    #[tokio::test]
    async fn session_mismatch_rejected() {
        let (tx_a, _) = broadcast::channel(64);
        let (tx_b, _) = broadcast::channel(64);

        let (mesh_a, _rx_a, _pe_a) = PeerMesh::bind(
            MeshConfig {
                peer_id: "alpha".to_string(),
                session_id: "session-1".to_string(),
            },
            tx_a.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let (mesh_b, _rx_b, _pe_b) = PeerMesh::bind(
            MeshConfig {
                peer_id: "beta".to_string(),
                session_id: "session-2".to_string(),
            },
            tx_b.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let addr_b = SocketAddr::from(([127, 0, 0, 1], mesh_b.local_port()));
        mesh_a.connect_peer(addr_b, "beta".to_string()).await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Connection should have been dropped due to session mismatch
        assert_eq!(mesh_a.peer_count().await, 0);
        assert_eq!(mesh_b.peer_count().await, 0);
    }

    #[tokio::test]
    async fn clock_calibration_near_zero_on_localhost() {
        // On the same machine, clock offset should be ~0ms
        let (tx_a, _) = broadcast::channel(64);
        let (tx_b, _) = broadcast::channel(64);

        let (mesh_a, _rx_a, _pe_a) = PeerMesh::bind(
            MeshConfig {
                peer_id: "cal-a".to_string(),
                session_id: "cal-session".to_string(),
            },
            tx_a.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let (_mesh_b, _rx_b, _pe_b) = PeerMesh::bind(
            MeshConfig {
                peer_id: "cal-b".to_string(),
                session_id: "cal-session".to_string(),
            },
            tx_b.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let addr_b = SocketAddr::from(([127, 0, 0, 1], _mesh_b.local_port()));
        mesh_a.connect_peer(addr_b, "cal-b".to_string()).await;

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert_eq!(mesh_a.peer_count().await, 1);

        // Verify offset is stored and near zero (same machine)
        let peers_lock = mesh_a.peers.lock().await;
        let peer = peers_lock.get("cal-b").expect("peer should be connected");
        assert!(
            peer.clock_offset_ms.abs() < 5,
            "on localhost, clock offset should be <5ms, got {}ms",
            peer.clock_offset_ms
        );
    }

    #[tokio::test]
    async fn clock_offset_adjusts_event_timestamp() {
        // Verify that received events have adjusted created_at_ms
        let (tx_a, _) = broadcast::channel(64);
        let (tx_b, _) = broadcast::channel(64);

        let (mesh_a, _rx_a, _pe_a) = PeerMesh::bind(
            MeshConfig {
                peer_id: "adj-a".to_string(),
                session_id: "adj-session".to_string(),
            },
            tx_a.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let (_mesh_b, mut rx_b, _pe_b) = PeerMesh::bind(
            MeshConfig {
                peer_id: "adj-b".to_string(),
                session_id: "adj-session".to_string(),
            },
            tx_b.subscribe(),
            empty_suppression(),
        )
        .await
        .unwrap();

        let addr_b = SocketAddr::from(([127, 0, 0, 1], _mesh_b.local_port()));
        mesh_a.connect_peer(addr_b, "adj-b".to_string()).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Send event from A → B
        let before = SyncEvent::now_ms();
        tx_a.send(make_event("adj-a", 1)).unwrap();

        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx_b.recv())
            .await
            .unwrap()
            .unwrap();
        let after = SyncEvent::now_ms();

        // After clock adjustment, the created_at_ms should still be
        // reasonable (between before and after on the local clock)
        assert!(
            received.created_at_ms >= before - 10 && received.created_at_ms <= after + 10,
            "adjusted timestamp {} should be between {} and {} (±10ms)",
            received.created_at_ms,
            before,
            after
        );
    }
}
