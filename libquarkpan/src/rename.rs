use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};

/// Builder for renaming a cloud file or folder by id.
pub struct RenameBuilder {
    inner: Arc<QuarkPanInner>,
    fid: Option<String>,
    file_name: Option<String>,
}

impl RenameBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            fid: None,
            file_name: None,
        }
    }

    /// Sets the target file or folder fid.
    pub fn fid(mut self, fid: impl Into<String>) -> Self {
        self.fid = Some(fid.into());
        self
    }

    /// Sets the new file or folder name.
    pub fn file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// Prepares the rename request.
    pub fn prepare(self) -> Result<RenameRequest> {
        Ok(RenameRequest {
            inner: self.inner,
            fid: self
                .fid
                .ok_or_else(|| QuarkPanError::missing_field("fid"))?,
            file_name: self
                .file_name
                .ok_or_else(|| QuarkPanError::missing_field("file_name"))?,
        })
    }
}

/// Prepared rename request.
pub struct RenameRequest {
    inner: Arc<QuarkPanInner>,
    fid: String,
    file_name: String,
}

impl RenameRequest {
    /// Sends the rename request.
    pub async fn request(self) -> Result<()> {
        self.inner.api.rename(&self.fid, &self.file_name).await
    }
}
