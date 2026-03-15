use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};
use crate::model::{BoxByteStream, DownloadInfo};

pub struct DownloadBuilder {
    inner: Arc<QuarkPanInner>,
    file_id: Option<String>,
    start_offset: Option<u64>,
}

impl DownloadBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            file_id: None,
            start_offset: None,
        }
    }

    /// Sets the file id to download.
    pub fn file_id(mut self, file_id: impl Into<String>) -> Self {
        self.file_id = Some(file_id.into());
        self
    }

    /// Starts reading the remote file from the given byte offset.
    pub fn start_offset(mut self, start_offset: u64) -> Self {
        self.start_offset = Some(start_offset);
        self
    }

    /// Prepares the download request.
    pub fn prepare(self) -> Result<DownloadRequest> {
        let file_id = self
            .file_id
            .ok_or_else(|| QuarkPanError::missing_field("file_id"))?;
        Ok(DownloadRequest {
            inner: self.inner,
            file_id,
            start_offset: self.start_offset,
        })
    }
}

pub struct DownloadRequest {
    inner: Arc<QuarkPanInner>,
    file_id: String,
    start_offset: Option<u64>,
}

impl DownloadRequest {
    /// Fetches the current download metadata, including the temporary url and md5 when available.
    pub async fn info(&self) -> Result<DownloadInfo> {
        self.inner.api.get_download_info(&self.file_id).await
    }

    /// Opens a byte stream for the target file.
    pub async fn stream(&self) -> Result<BoxByteStream> {
        self.inner
            .api
            .download_stream(&self.file_id, self.start_offset)
            .await
    }
}
