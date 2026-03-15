use std::sync::Arc;

use bytes::Bytes;
use futures_core::Stream;
use futures_util::StreamExt;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};
use crate::model::{
    UpAuthAndCommitRequest, UpPartMethodRequest, UploadComplete, UploadPrepareResult, UploadResume,
    UploadResumeState, UploadSession,
};

pub struct UploadBuilder {
    inner: Arc<QuarkPanInner>,
    parent_folder: String,
    name: Option<String>,
    size: Option<u64>,
    md5: Option<String>,
    sha1: Option<String>,
}

impl UploadBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            parent_folder: "0".to_string(),
            name: None,
            size: None,
            md5: None,
            sha1: None,
        }
    }

    /// Sets the parent folder id. Defaults to the root folder `"0"`.
    pub fn parent_folder(mut self, parent_folder: impl Into<String>) -> Self {
        self.parent_folder = parent_folder.into();
        self
    }

    /// Sets the file name to create in Quark Drive.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the total file size in bytes.
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the lowercase MD5 hex digest of the file content.
    pub fn md5(mut self, md5: impl Into<String>) -> Self {
        self.md5 = Some(md5.into());
        self
    }

    /// Sets the lowercase SHA-1 hex digest of the file content.
    pub fn sha1(mut self, sha1: impl Into<String>) -> Self {
        self.sha1 = Some(sha1.into());
        self
    }

    /// Validates the upload parameters, sends the upload preflight request,
    /// and returns either a rapid-upload result or a resumable upload session.
    pub async fn prepare(self) -> Result<UploadPrepareResult> {
        let name = self
            .name
            .ok_or_else(|| QuarkPanError::missing_field("name"))?;
        let size = self
            .size
            .ok_or_else(|| QuarkPanError::missing_field("size"))?;
        let md5 = self
            .md5
            .ok_or_else(|| QuarkPanError::missing_field("md5"))?;
        let sha1 = self
            .sha1
            .ok_or_else(|| QuarkPanError::missing_field("sha1"))?;
        let pre = self
            .inner
            .api
            .up_pre(&name, size, &self.parent_folder)
            .await?;
        if pre.data.finish {
            return Ok(UploadPrepareResult::RapidUploaded {
                file_id: pre.data.fid,
            });
        }
        let task_id = pre.data.task_id.clone();
        let hash = self.inner.api.up_hash(&md5, &sha1, &task_id).await?;
        if hash.data.finish {
            return Ok(UploadPrepareResult::RapidUploaded {
                file_id: pre.data.fid,
            });
        }
        let upload_id = pre.data.upload_id.ok_or_else(|| {
            QuarkPanError::invalid_argument("missing upload_id in prepare response")
        })?;
        Ok(UploadPrepareResult::NeedUpload(UploadSession {
            api: self.inner.api.clone(),
            file_id: pre.data.fid,
            size,
            mime_type: if pre.data.format_type.is_empty() {
                "application/octet-stream".to_string()
            } else {
                pre.data.format_type
            },
            part_size: pre.metadata.part_size,
            auth_info: pre.data.auth_info,
            callback: pre.data.callback,
            bucket: pre.data.bucket,
            obj_key: pre.data.obj_key,
            upload_id,
            upload_url: pre
                .data
                .upload_url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .to_string(),
            task_id,
        }))
    }
}

impl UploadBuilder {
    /// Recreates an upload session from a previously exported resume payload.
    pub fn resume(self, resume: UploadResume) -> UploadSession {
        UploadSession {
            api: self.inner.api.clone(),
            file_id: resume.file_id,
            size: resume.size,
            mime_type: resume.mime_type,
            part_size: resume.part_size,
            auth_info: resume.auth_info,
            callback: resume.callback,
            bucket: resume.bucket,
            obj_key: resume.obj_key,
            upload_id: resume.upload_id,
            upload_url: resume.upload_url,
            task_id: resume.task_id,
        }
    }
}

pub(crate) async fn upload_stream<S, E>(session: UploadSession, stream: S) -> Result<UploadComplete>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
    E: Into<QuarkPanError>,
{
    upload_stream_resumable(session, stream, UploadResumeState::default(), |_state| {
        Ok(())
    })
    .await
}

pub(crate) async fn upload_stream_resumable<S, E, F>(
    session: UploadSession,
    stream: S,
    mut state: UploadResumeState,
    mut on_part_uploaded: F,
) -> Result<UploadComplete>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
    E: Into<QuarkPanError>,
    F: FnMut(&UploadResumeState) -> Result<()> + Send + 'static,
{
    let mut stream = Box::pin(stream);
    let part_size = session.part_size as usize;
    if part_size == 0 {
        return Err(QuarkPanError::invalid_argument(
            "part_size must be greater than 0",
        ));
    }
    let mut buffer = bytes::BytesMut::new();
    let mut sent: u64 = ((state.next_part_number.saturating_sub(1)) as u64) * session.part_size;
    let mut part_number: u32 = if state.next_part_number == 0 {
        1
    } else {
        state.next_part_number
    };

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Into::into)?;
        sent += chunk.len() as u64;
        if sent > session.size {
            return Err(QuarkPanError::invalid_argument(
                "stream produced more bytes than declared size",
            ));
        }
        buffer.extend_from_slice(&chunk);
        while buffer.len() >= part_size {
            let bytes = buffer.split_to(part_size).freeze();
            let etag = upload_part(&session, part_number, bytes).await?;
            state.part_etags.push(etag);
            part_number += 1;
            state.next_part_number = part_number;
            on_part_uploaded(&state)?;
        }
    }

    if !buffer.is_empty() {
        let etag = upload_part(&session, part_number, buffer.freeze()).await?;
        state.part_etags.push(etag);
        part_number += 1;
        state.next_part_number = part_number;
        on_part_uploaded(&state)?;
    }

    if sent != session.size {
        return Err(QuarkPanError::invalid_argument(format!(
            "stream size mismatch: declared {}, actual {}",
            session.size, sent
        )));
    }

    session
        .api
        .up_auth_and_commit(UpAuthAndCommitRequest {
            md5s: state.part_etags,
            callback: session.callback.clone(),
            bucket: session.bucket.clone(),
            obj_key: session.obj_key.clone(),
            upload_id: session.upload_id.clone(),
            auth_info: session.auth_info.clone(),
            task_id: session.task_id.clone(),
            upload_url: session.upload_url.clone(),
        })
        .await?;
    session
        .api
        .finish(&session.obj_key, &session.task_id)
        .await?;
    Ok(UploadComplete {
        file_id: session.file_id,
        rapid_upload: false,
    })
}

async fn upload_part(session: &UploadSession, part_number: u32, bytes: Bytes) -> Result<String> {
    let utc_time = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let auth_meta = session.api.up_part_auth_meta(
        &session.mime_type,
        &utc_time,
        &session.bucket,
        &session.obj_key,
        part_number,
        &session.upload_id,
    );
    let auth = session
        .api
        .auth(&session.auth_info, &auth_meta, &session.task_id)
        .await?;
    session
        .api
        .up_part(UpPartMethodRequest {
            auth_key: auth.data.auth_key,
            mime_type: session.mime_type.clone(),
            utc_time,
            bucket: session.bucket.clone(),
            upload_url: session.upload_url.clone(),
            obj_key: session.obj_key.clone(),
            part_number,
            upload_id: session.upload_id.clone(),
            part_bytes: bytes,
        })
        .await
}
