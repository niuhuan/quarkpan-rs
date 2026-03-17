mod api;
mod download;
mod error;
mod folder;
mod list;
mod model;
mod rename;
mod transfer;
mod upload;

pub use crate::download::{DownloadBuilder, DownloadRequest};
pub use crate::error::{QuarkPanError, Result};
pub use crate::folder::CreateFolderBuilder;
pub use crate::list::{ListBuilder, ListRequest};
pub use crate::model::{
    DownloadInfo, Fid, ListPage, QuarkEntry, UploadComplete, UploadPrepareResult, UploadResume,
    UploadResumeState, UploadSession,
};
pub use crate::rename::{RenameBuilder, RenameRequest};
pub use crate::transfer::{ProgressStream, TransferControl, TransferProgress};
pub use crate::upload::UploadBuilder;

use std::sync::Arc;

use api::{ApiClient, ApiConfig};
use dashmap::DashMap;

#[derive(Clone)]
pub struct QuarkPan {
    inner: Arc<QuarkPanInner>,
}

struct QuarkPanInner {
    api: ApiClient,
}

impl QuarkPan {
    /// Creates a builder for constructing a [`QuarkPan`] client.
    pub fn builder() -> QuarkPanBuilder {
        QuarkPanBuilder::default()
    }

    /// Creates a builder for downloading by fid.
    pub fn download(&self) -> DownloadBuilder {
        DownloadBuilder::new(self.inner.clone())
    }

    /// Creates a builder for preparing a file upload.
    pub fn upload(&self) -> UploadBuilder {
        UploadBuilder::new(self.inner.clone())
    }

    /// Creates a builder for creating a folder in Quark Drive.
    pub fn create_folder(&self) -> CreateFolderBuilder {
        CreateFolderBuilder::new(self.inner.clone())
    }

    /// Creates a builder for listing entries by parent directory fid.
    pub fn list(&self) -> ListBuilder {
        ListBuilder::new(self.inner.clone())
    }

    /// Creates a builder for renaming an entry by fid.
    pub fn rename(&self) -> RenameBuilder {
        RenameBuilder::new(self.inner.clone())
    }

    /// Deletes an entry by fid.
    pub async fn delete(&self, fid: &str) -> Result<()> {
        self.inner.api.delete(fid).await
    }
}

#[derive(Default)]
pub struct QuarkPanBuilder {
    cookie: Option<String>,
    api_base_url: Option<String>,
}

impl QuarkPanBuilder {
    /// Sets the Quark cookie used for all authenticated requests.
    pub fn cookie(mut self, cookie: impl Into<String>) -> Self {
        self.cookie = Some(cookie.into());
        self
    }

    /// Overrides the default Quark Drive API base url.
    pub fn api_base_url(mut self, api_base_url: impl Into<String>) -> Self {
        self.api_base_url = Some(api_base_url.into());
        self
    }

    /// Prepares the final [`QuarkPan`] client.
    pub fn prepare(self) -> Result<QuarkPan> {
        let cookie = self
            .cookie
            .ok_or_else(|| QuarkPanError::missing_field("cookie"))?;
        let cookie_map = Arc::new(DashMap::new());
        for pair in cookie.split(';') {
            if let Some((k, v)) = pair.trim().split_once('=') {
                cookie_map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        if cookie_map.is_empty() {
            return Err(QuarkPanError::invalid_argument(
                "cookie must contain at least one key=value pair",
            ));
        }
        let api = ApiClient::new(ApiConfig {
            api_base_url: self
                .api_base_url
                .unwrap_or_else(|| "https://drive.quark.cn".to_string()),
            cookie: cookie_map,
        })?;
        Ok(QuarkPan {
            inner: Arc::new(QuarkPanInner { api }),
        })
    }
}
