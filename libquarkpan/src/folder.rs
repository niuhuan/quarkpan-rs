use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};

pub struct CreateFolderBuilder {
    inner: Arc<QuarkPanInner>,
    pdir_fid: String,
    file_name: Option<String>,
}

impl CreateFolderBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            pdir_fid: "0".to_string(),
            file_name: None,
        }
    }

    /// Sets the parent directory fid. Defaults to the root folder `"0"`.
    pub fn pdir_fid(mut self, pdir_fid: impl Into<String>) -> Self {
        self.pdir_fid = pdir_fid.into();
        self
    }

    /// Sets the folder name to create.
    pub fn file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = Some(file_name.into());
        self
    }

    /// Prepares the folder creation request.
    pub fn prepare(self) -> Result<CreateFolderRequest> {
        let file_name = self
            .file_name
            .ok_or_else(|| QuarkPanError::missing_field("file_name"))?;
        Ok(CreateFolderRequest {
            inner: self.inner,
            pdir_fid: self.pdir_fid,
            file_name,
        })
    }
}

pub struct CreateFolderRequest {
    inner: Arc<QuarkPanInner>,
    pdir_fid: String,
    file_name: String,
}

impl CreateFolderRequest {
    /// Sends the prepared folder creation request and returns the new fid.
    pub async fn request(self) -> Result<String> {
        self.inner
            .api
            .create_folder(&self.pdir_fid, &self.file_name)
            .await
    }
}
