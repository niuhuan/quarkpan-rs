use std::sync::Arc;

use crate::QuarkPanInner;
use crate::error::Result;
use crate::model::ListPage;

/// Builder for listing the contents of a Quark folder.
pub struct ListBuilder {
    inner: Arc<QuarkPanInner>,
    folder_id: String,
    page: u32,
    size: u32,
}

impl ListBuilder {
    pub(crate) fn new(inner: Arc<QuarkPanInner>) -> Self {
        Self {
            inner,
            folder_id: "0".to_string(),
            page: 1,
            size: 100,
        }
    }

    /// Sets the folder id to list. Defaults to the root folder `"0"`.
    pub fn folder_id(mut self, folder_id: impl Into<String>) -> Self {
        self.folder_id = folder_id.into();
        self
    }

    /// Sets the page number. Defaults to `1`.
    pub fn page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    /// Sets the page size. Defaults to `100`.
    pub fn size(mut self, size: u32) -> Self {
        self.size = size;
        self
    }

    /// Prepares the list request.
    pub fn prepare(self) -> Result<ListRequest> {
        Ok(ListRequest {
            inner: self.inner,
            folder_id: self.folder_id,
            page: self.page,
            size: self.size,
        })
    }
}

/// Prepared list request.
pub struct ListRequest {
    inner: Arc<QuarkPanInner>,
    folder_id: String,
    page: u32,
    size: u32,
}

impl ListRequest {
    /// Sends the request and returns the current page of entries.
    pub async fn request(&self) -> Result<ListPage> {
        self.inner
            .api
            .list_folder(&self.folder_id, self.page, self.size)
            .await
    }
}
