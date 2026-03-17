use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};
use crate::model::{BoxByteStream, DownloadInfo};

pub struct DownloadBuilder {
    inner: Arc<QuarkPanInner>,
    fid: Option<String>,
    start_offset: Option<u64>,
}

impl DownloadBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            fid: None,
            start_offset: None,
        }
    }

    /// Sets the target fid to download.
    pub fn fid(mut self, fid: impl Into<String>) -> Self {
        self.fid = Some(fid.into());
        self
    }

    /// Starts reading the remote file from the given byte offset.
    pub fn start_offset(mut self, start_offset: u64) -> Self {
        self.start_offset = Some(start_offset);
        self
    }

    /// Prepares the download request.
    pub fn prepare(self) -> Result<DownloadRequest> {
        let fid = self
            .fid
            .ok_or_else(|| QuarkPanError::missing_field("fid"))?;
        Ok(DownloadRequest {
            inner: self.inner,
            fid,
            start_offset: self.start_offset,
        })
    }
}

pub struct DownloadRequest {
    inner: Arc<QuarkPanInner>,
    fid: String,
    start_offset: Option<u64>,
}

impl DownloadRequest {
    /// Fetches the current download metadata, including the temporary url and md5 when available.
    pub async fn info(&self) -> Result<DownloadInfo> {
        self.inner.api.get_download_info(&self.fid).await
    }

    /// Opens a byte stream for the target file.
    pub async fn stream(&self) -> Result<BoxByteStream> {
        self.inner
            .api
            .download_stream(&self.fid, self.start_offset)
            .await
    }
}
