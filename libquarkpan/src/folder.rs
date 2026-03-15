use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::{QuarkPanError, Result};

pub struct CreateFolderBuilder {
    inner: Arc<QuarkPanInner>,
    parent_folder: String,
    name: Option<String>,
}

impl CreateFolderBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            parent_folder: "0".to_string(),
            name: None,
        }
    }

    /// Sets the parent folder id. Defaults to the root folder `"0"`.
    pub fn parent_folder(mut self, parent_folder: impl Into<String>) -> Self {
        self.parent_folder = parent_folder.into();
        self
    }

    /// Sets the folder name to create.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Prepares the folder creation request.
    pub fn prepare(self) -> Result<CreateFolderRequest> {
        let name = self
            .name
            .ok_or_else(|| QuarkPanError::missing_field("name"))?;
        Ok(CreateFolderRequest {
            inner: self.inner,
            parent_folder: self.parent_folder,
            name,
        })
    }
}

pub struct CreateFolderRequest {
    inner: Arc<QuarkPanInner>,
    parent_folder: String,
    name: String,
}

impl CreateFolderRequest {
    /// Sends the prepared folder creation request and returns the new folder id.
    pub async fn request(self) -> Result<String> {
        self.inner
            .api
            .create_folder(&self.parent_folder, &self.name)
            .await
    }
}
