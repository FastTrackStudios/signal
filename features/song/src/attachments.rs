//! Wiring between a [`Song`]'s binary media and Task's content-addressed
//! [`AttachmentService`](attachments_proto::AttachmentService).
//!
//! This module is gated behind the additive **`attachments`** cargo feature
//! so the default `song` build stays dependency-light (schema + folder I/O
//! only). Enabling it pulls in `attachments-proto` (default-features off — no
//! vox), giving the client-side upload/resolve flow described in
//! `plans/decentralized-foundation.md` §13 Phase 7:
//!
//! 1. [`upload_arrangement_attachment`] — `initiate_upload` → PUT the bytes →
//!    `complete_upload`, then record the resulting [`AttachmentRef`]
//!    (content-hash id + sha256 + kind) onto the arrangement.
//! 2. [`resolve_attachment_url`] / [`resolve_arrangement_urls`] — turn an
//!    arrangement's recorded refs into short-lived
//!    [`SignedDownloadUrl`]s for consumers (playback, bundles).
//!
//! Everything works against the [`AttachmentService`] **trait**, so no running
//! server is required — callers inject any client (a real vox client, or a
//! mock in tests). The raw byte-PUT step is likewise abstracted behind the
//! [`BlobPut`] trait: in production that's an HTTP client PUT-ing to the
//! signed `upload_url`; in tests a mock that captures + hashes the bytes.

use std::path::Path;

use attachments_proto::{
    AttachmentError, AttachmentService, CompleteUpload, ContentHashArg, InitiateUpload,
    SignedDownloadUrl, UploadTicket,
};
use thiserror::Error;

use crate::model::{ArrangementId, AttachmentRef, Song};

/// Transport for the raw byte-PUT step of an upload.
///
/// [`AttachmentService::initiate_upload`] returns a signed [`UploadTicket`]
/// with a PUT `upload_url`; something has to actually push the bytes there.
/// The server hashes the body as it reads and returns the computed sha256 —
/// which [`AttachmentService::complete_upload`] then requires. This trait
/// captures exactly that step: given the ticket + bytes, PUT them and return
/// the content hash (sha256 hex).
///
/// Kept out of `song`'s dependency graph on purpose: production wires an HTTP
/// client here, tests wire an in-memory mock, and neither pins a concrete
/// networking dep into this schema crate.
pub trait BlobPut {
    /// PUT `bytes` to the ticket's `upload_url`, returning the server-computed
    /// sha256 hex content hash.
    fn put_bytes(
        &self,
        ticket: &UploadTicket,
        bytes: &[u8],
    ) -> impl std::future::Future<Output = Result<String, AttachmentError>>;
}

/// Errors from the attachment wiring.
#[derive(Debug, Error)]
pub enum AttachError {
    /// Reading the local file to upload failed.
    #[error("io reading `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The referenced arrangement isn't part of the song.
    #[error("arrangement `{0}` not found in song")]
    NoArrangement(ArrangementId),

    /// An [`AttachmentRef`] had neither a `sha256` nor a usable `id` to
    /// address the blob by.
    #[error("attachment ref has no content hash to resolve")]
    NoContentHash,

    /// The underlying [`AttachmentService`] (or blob transport) failed.
    #[error(transparent)]
    Service(#[from] AttachmentError),
}

/// Upload a local binary file as an attachment for `arrangement`, recording
/// the resulting [`AttachmentRef`] onto that arrangement in `song`.
///
/// Runs the full three-step flow against the injected `service` +
/// `blob_put`: `initiate_upload` (scoped to the song's id as `doc_id`) → PUT
/// the bytes → `complete_upload`. The committed blob is content-addressed, so
/// the recorded ref's `id` **and** `sha256` are both the server-computed
/// content hash; `path` is left `None` (the bytes live in the blob store, not
/// in-folder). `kind` overrides the hint derived from the file's mime type.
///
/// Returns a clone of the recorded ref.
///
/// # Errors
/// - [`AttachError::Io`] if `local_file` can't be read.
/// - [`AttachError::NoArrangement`] if `arrangement` isn't in `song`.
/// - [`AttachError::Service`] if the service or blob PUT fails.
pub async fn upload_arrangement_attachment<S, P>(
    service: &S,
    blob_put: &P,
    song: &mut Song,
    arrangement: ArrangementId,
    local_file: &Path,
    kind: Option<String>,
) -> Result<AttachmentRef, AttachError>
where
    S: AttachmentService,
    P: BlobPut,
{
    // Fail fast if the target arrangement doesn't exist, before doing I/O or
    // touching the service.
    if song.arrangement(arrangement).is_none() {
        return Err(AttachError::NoArrangement(arrangement));
    }

    let bytes = std::fs::read(local_file).map_err(|source| AttachError::Io {
        path: local_file.display().to_string(),
        source,
    })?;
    let filename = local_file
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment".to_string());
    let mime_type = guess_mime(&filename).to_string();

    let ticket = service
        .initiate_upload(InitiateUpload {
            doc_id: song.id.to_string(),
            filename,
            mime_type: mime_type.clone(),
            size_bytes: bytes.len() as u64,
        })
        .await?;

    let content_hash = blob_put.put_bytes(&ticket, &bytes).await?;

    let meta = service
        .complete_upload(CompleteUpload {
            upload_id: ticket.upload_id,
            content_hash,
        })
        .await?;

    let att = AttachmentRef {
        id: meta.content_hash.clone(),
        path: None,
        sha256: Some(meta.content_hash.clone()),
        kind: Some(kind.unwrap_or_else(|| kind_from_mime(&meta.mime_type).to_string())),
    };

    // Already checked present above; find again to record.
    let arr = song
        .arrangements
        .iter_mut()
        .find(|a| a.id == arrangement)
        .ok_or(AttachError::NoArrangement(arrangement))?;
    arr.attachment_refs.push(att.clone());
    Ok(att)
}

/// Resolve a single [`AttachmentRef`] to a short-lived signed download URL.
///
/// Addresses the blob by the ref's `sha256`, falling back to `id` (which for
/// content-addressed attachments is the same hash).
///
/// # Errors
/// - [`AttachError::NoContentHash`] if the ref carries no usable hash.
/// - [`AttachError::Service`] if the service fails (e.g. `NotFound`).
pub async fn resolve_attachment_url<S: AttachmentService>(
    service: &S,
    att: &AttachmentRef,
) -> Result<SignedDownloadUrl, AttachError> {
    let content_hash = att
        .sha256
        .clone()
        .or_else(|| (!att.id.is_empty()).then(|| att.id.clone()))
        .ok_or(AttachError::NoContentHash)?;
    Ok(service
        .get_download_url(ContentHashArg { content_hash })
        .await?)
}

/// Resolve every attachment ref on `arrangement` to a signed download URL.
///
/// Returns one entry per ref, pairing the ref with its resolution result so a
/// single failure doesn't sink the batch.
///
/// # Errors
/// [`AttachError::NoArrangement`] if `arrangement` isn't in `song`.
pub async fn resolve_arrangement_urls<'a, S: AttachmentService>(
    service: &S,
    song: &'a Song,
    arrangement: ArrangementId,
) -> Result<Vec<(&'a AttachmentRef, Result<SignedDownloadUrl, AttachError>)>, AttachError> {
    let arr = song
        .arrangement(arrangement)
        .ok_or(AttachError::NoArrangement(arrangement))?;
    let mut out = Vec::with_capacity(arr.attachment_refs.len());
    for att in &arr.attachment_refs {
        let res = resolve_attachment_url(service, att).await;
        out.push((att, res));
    }
    Ok(out)
}

/// Best-effort mime type from a filename extension. Deliberately tiny — a
/// dependency-free hint, not a full mime database.
fn guess_mime(filename: &str) -> &'static str {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "aif" | "aiff" => "audio/aiff",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Coarse `kind` hint (matching [`AttachmentRef::kind`]'s open set) from a
/// mime type: `"audio"`, `"pdf"`, `"image"`, else `"file"`.
fn kind_from_mime(mime: &str) -> &'static str {
    if mime.starts_with("audio/") {
        "audio"
    } else if mime == "application/pdf" {
        "pdf"
    } else if mime.starts_with("image/") {
        "image"
    } else {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use attachments_proto::AttachmentMeta;
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    use super::*;
    use crate::key::Key;
    use crate::model::{Arrangement, PartsManifest};

    /// In-memory stand-in for the real server: implements both
    /// [`AttachmentService`] and [`BlobPut`]. `initiate_upload` records a
    /// pending session; `put_bytes` hashes (sha256) + stores the blob and
    /// returns the hash; `complete_upload` commits metadata; `get_download_url`
    /// hands back a deterministic fake signed URL for any stored hash.
    // `Clone` because the `vox::service` macro makes `AttachmentService: Clone`
    // a supertrait; the shared state lives behind an `Arc` so clones observe
    // the same map.
    #[derive(Default, Clone)]
    struct MockAttachments {
        state: Arc<Mutex<MockState>>,
    }

    #[derive(Default)]
    struct MockState {
        /// upload_id -> the InitiateUpload request.
        pending: HashMap<Uuid, InitiateUpload>,
        /// content_hash -> stored bytes.
        blobs: HashMap<String, Vec<u8>>,
        /// content_hash -> committed metadata (hash, filename, mime, size, doc).
        committed: HashMap<String, (String, String, u64, String)>,
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    impl AttachmentService for MockAttachments {
        async fn initiate_upload(
            &self,
            req: InitiateUpload,
        ) -> Result<UploadTicket, AttachmentError> {
            let upload_id = Uuid::new_v4();
            let ticket = UploadTicket {
                upload_id,
                upload_url: format!("mock://blobs/upload?upload_id={upload_id}"),
                method: "PUT".to_string(),
                expires_unix: 0,
            };
            self.state.lock().unwrap().pending.insert(upload_id, req);
            Ok(ticket)
        }

        async fn complete_upload(
            &self,
            req: CompleteUpload,
        ) -> Result<AttachmentMeta, AttachmentError> {
            let mut st = self.state.lock().unwrap();
            let pending = st
                .pending
                .remove(&req.upload_id)
                .ok_or(AttachmentError::NotFound)?;
            if !st.blobs.contains_key(&req.content_hash) {
                return Err(AttachmentError::InvalidInput(
                    "bytes never PUT for that hash".to_string(),
                ));
            }
            let size = pending.size_bytes;
            st.committed.insert(
                req.content_hash.clone(),
                (
                    pending.filename.clone(),
                    pending.mime_type.clone(),
                    size,
                    pending.doc_id.clone(),
                ),
            );
            Ok(AttachmentMeta {
                content_hash: req.content_hash,
                filename: pending.filename,
                mime_type: pending.mime_type,
                size_bytes: size,
                doc_id: pending.doc_id,
            })
        }

        async fn get_download_url(
            &self,
            arg: ContentHashArg,
        ) -> Result<SignedDownloadUrl, AttachmentError> {
            let st = self.state.lock().unwrap();
            if !st.committed.contains_key(&arg.content_hash) {
                return Err(AttachmentError::NotFound);
            }
            Ok(SignedDownloadUrl {
                url: format!("mock://blobs/download/{}?token=fake", arg.content_hash),
                expires_unix: 9_999_999_999,
            })
        }
    }

    impl BlobPut for MockAttachments {
        async fn put_bytes(
            &self,
            _ticket: &UploadTicket,
            bytes: &[u8],
        ) -> Result<String, AttachmentError> {
            let hash = sha256_hex(bytes);
            self.state
                .lock()
                .unwrap()
                .blobs
                .insert(hash.clone(), bytes.to_vec());
            Ok(hash)
        }
    }

    fn one_arrangement_song() -> (Song, ArrangementId) {
        let arr_id = Uuid::new_v4();
        let arr = Arrangement {
            id: arr_id,
            name: "Default".to_string(),
            key: Key::c_major(),
            tempo_bpm: None,
            time_signature: None,
            chart_ref: None,
            parts: PartsManifest::default(),
            attachment_refs: vec![],
        };
        let song = Song {
            id: Uuid::new_v4(),
            title: "Test Song".to_string(),
            tags: vec![],
            default_arrangement: arr_id,
            arrangements: vec![arr],
        };
        (song, arr_id)
    }

    #[tokio::test]
    async fn uploads_wav_and_pdf_then_resolves_signed_urls() {
        let mock = MockAttachments::default();
        let (mut song, arr_id) = one_arrangement_song();

        let dir = tempfile::tempdir().unwrap();
        let wav_path = dir.path().join("stem.wav");
        let pdf_path = dir.path().join("chart.pdf");
        let wav_bytes = b"RIFF....WAVEfake-audio-bytes".to_vec();
        let pdf_bytes = b"%PDF-1.7 fake-chart-bytes".to_vec();
        std::fs::write(&wav_path, &wav_bytes).unwrap();
        std::fs::write(&pdf_path, &pdf_bytes).unwrap();

        // (a) upload both files; refs get recorded onto the arrangement.
        let wav_ref =
            upload_arrangement_attachment(&mock, &mock, &mut song, arr_id, &wav_path, None)
                .await
                .expect("wav upload");
        let pdf_ref =
            upload_arrangement_attachment(&mock, &mock, &mut song, arr_id, &pdf_path, None)
                .await
                .expect("pdf upload");

        // Recorded by content hash (== sha256 of the bytes), kind derived.
        assert_eq!(
            wav_ref.sha256.as_deref(),
            Some(sha256_hex(&wav_bytes).as_str())
        );
        assert_eq!(wav_ref.id, sha256_hex(&wav_bytes));
        assert_eq!(wav_ref.kind.as_deref(), Some("audio"));
        assert!(wav_ref.path.is_none());

        assert_eq!(
            pdf_ref.sha256.as_deref(),
            Some(sha256_hex(&pdf_bytes).as_str())
        );
        assert_eq!(pdf_ref.kind.as_deref(), Some("pdf"));

        let arr = song.arrangement(arr_id).unwrap();
        assert_eq!(
            arr.attachment_refs.len(),
            2,
            "both refs recorded on arrangement"
        );

        // (b) resolve the recorded refs to signed download URLs.
        let resolved = resolve_arrangement_urls(&mock, &song, arr_id)
            .await
            .expect("resolve");
        assert_eq!(resolved.len(), 2);
        for (att, res) in resolved {
            let url = res.expect("signed url");
            assert!(
                url.url.contains(att.sha256.as_ref().unwrap()),
                "signed url addresses the ref's content hash: {}",
                url.url
            );
        }
    }

    #[tokio::test]
    async fn explicit_kind_overrides_mime_hint() {
        let mock = MockAttachments::default();
        let (mut song, arr_id) = one_arrangement_song();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mystery.bin");
        std::fs::write(&path, b"raw").unwrap();

        let att = upload_arrangement_attachment(
            &mock,
            &mock,
            &mut song,
            arr_id,
            &path,
            Some("stem".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(att.kind.as_deref(), Some("stem"));
    }

    #[tokio::test]
    async fn upload_to_unknown_arrangement_errors() {
        let mock = MockAttachments::default();
        let (mut song, _arr_id) = one_arrangement_song();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.wav");
        std::fs::write(&path, b"x").unwrap();

        let err =
            upload_arrangement_attachment(&mock, &mock, &mut song, Uuid::new_v4(), &path, None)
                .await
                .unwrap_err();
        assert!(matches!(err, AttachError::NoArrangement(_)));
    }

    #[tokio::test]
    async fn resolve_unknown_hash_reports_not_found() {
        let mock = MockAttachments::default();
        let att = AttachmentRef {
            id: "nope".to_string(),
            path: None,
            sha256: Some("0000".to_string()),
            kind: None,
        };
        let err = resolve_attachment_url(&mock, &att).await.unwrap_err();
        assert!(matches!(
            err,
            AttachError::Service(AttachmentError::NotFound)
        ));
    }
}
