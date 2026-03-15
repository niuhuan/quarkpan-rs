use std::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

use crate::error::Result;

pub type BoxByteStream =
    Pin<Box<dyn Stream<Item = std::result::Result<Bytes, crate::QuarkPanError>> + Send>>;

/// Quark folder id. The root folder id is `"0"`.
pub type FolderId = String;

#[derive(Debug, Clone, Deserialize)]
pub struct Response<T, U> {
    pub status: u32,
    pub code: u32,
    pub message: String,
    #[allow(dead_code)]
    pub timestamp: u64,
    pub data: T,
    pub metadata: U,
}

#[derive(Debug, Serialize, Clone)]
pub struct GetFilesDownloadUrlsRequest {
    pub fids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkEntry {
    pub fid: String,
    pub file_name: String,
    pub pdir_fid: String,
    #[serde(default)]
    pub size: u64,
    pub format_type: String,
    pub status: u8,
    pub created_at: u64,
    pub updated_at: u64,
    pub dir: bool,
    pub file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPage {
    pub entries: Vec<QuarkEntry>,
    pub page: u32,
    pub size: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDownloadUrlItem {
    pub fid: String,
    pub download_url: String,
    #[serde(default)]
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub file_id: String,
    pub download_url: String,
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmptyMetadata {}

#[derive(Debug, Clone, Deserialize)]
pub struct EmptyData {}

#[derive(Debug, Serialize, Clone)]
pub struct CreateFolderRequest {
    pub pdir_fid: String,
    pub file_name: String,
    pub dir_path: String,
    pub dir_init_lock: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateFolderData {
    #[allow(dead_code)]
    pub finish: bool,
    pub fid: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpPreRequest {
    pub file_name: String,
    pub size: u64,
    pub pdir_fid: String,
    pub format_type: String,
    pub ccp_hash_update: bool,
    pub l_created_at: u64,
    pub l_updated_at: u64,
    pub parallel_upload: bool,
    pub dir_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpPreResponseData {
    pub finish: bool,
    pub task_id: String,
    pub upload_id: Option<String>,
    pub auth_info: String,
    pub upload_url: String,
    pub obj_key: String,
    pub fid: String,
    pub bucket: String,
    pub format_type: String,
    #[allow(dead_code)]
    pub auth_info_expried: u64,
    pub callback: Callback,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpPreResponseMetaData {
    pub part_size: u64,
    #[allow(dead_code)]
    pub part_thread: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpHashRequest {
    pub md5: String,
    pub sha1: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpHashResponseData {
    pub finish: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct AuthRequest {
    pub auth_info: String,
    pub auth_meta: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponseData {
    pub auth_key: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct Callback {
    #[serde(rename = "callbackUrl")]
    pub callback_url: String,
    #[serde(rename = "callbackBody")]
    pub callback_body: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct FinishRequest {
    pub obj_key: String,
    pub task_id: String,
}

#[derive(Debug, Serialize, Clone, Deserialize)]
pub struct UpAuthAndCommitRequest {
    pub md5s: Vec<String>,
    pub callback: Callback,
    pub bucket: String,
    pub obj_key: String,
    pub upload_id: String,
    pub auth_info: String,
    pub task_id: String,
    pub upload_url: String,
}

#[derive(Debug)]
pub struct UpPartMethodRequest {
    pub auth_key: String,
    pub mime_type: String,
    pub utc_time: String,
    pub bucket: String,
    pub upload_url: String,
    pub obj_key: String,
    pub part_number: u32,
    pub upload_id: String,
    pub part_bytes: Bytes,
}

pub type GetFilesDownloadUrlsResponse = Response<Vec<FileDownloadUrlItem>, EmptyMetadata>;
pub type CreateFolderResponse = Response<CreateFolderData, EmptyMetadata>;
pub type UpPreResponse = Response<UpPreResponseData, UpPreResponseMetaData>;
pub type UpHashResponse = Response<UpHashResponseData, EmptyMetadata>;
pub type AuthResponse = Response<AuthResponseData, EmptyMetadata>;
pub type FinishResponse = Response<EmptyData, EmptyMetadata>;
pub type ListFolderResponse = Response<ListFolderData, ListFolderMetadata>;

#[derive(Debug, Clone, Deserialize)]
pub struct ListFolderData {
    pub list: Vec<QuarkEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListFolderMetadata {
    #[serde(rename = "_total")]
    pub total: u32,
    #[serde(rename = "_count")]
    pub count: u32,
    #[serde(rename = "_page")]
    pub page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResume {
    pub file_id: String,
    pub size: u64,
    pub mime_type: String,
    pub part_size: u64,
    pub auth_info: String,
    pub callback: Callback,
    pub bucket: String,
    pub obj_key: String,
    pub upload_id: String,
    pub upload_url: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UploadResumeState {
    pub next_part_number: u32,
    pub part_etags: Vec<String>,
}

#[derive(Clone)]
pub struct UploadSession {
    pub(crate) api: crate::api::ApiClient,
    pub(crate) file_id: String,
    pub(crate) size: u64,
    pub(crate) mime_type: String,
    pub(crate) part_size: u64,
    pub(crate) auth_info: String,
    pub(crate) callback: Callback,
    pub(crate) bucket: String,
    pub(crate) obj_key: String,
    pub(crate) upload_id: String,
    pub(crate) upload_url: String,
    pub(crate) task_id: String,
}

impl UploadSession {
    /// Returns the target file id created during upload preparation.
    pub fn file_id(&self) -> &str {
        &self.file_id
    }

    /// Exports the upload state so it can be resumed by another process later.
    pub fn to_resume(&self) -> UploadResume {
        UploadResume {
            file_id: self.file_id.clone(),
            size: self.size,
            mime_type: self.mime_type.clone(),
            part_size: self.part_size,
            auth_info: self.auth_info.clone(),
            callback: self.callback.clone(),
            bucket: self.bucket.clone(),
            obj_key: self.obj_key.clone(),
            upload_id: self.upload_id.clone(),
            upload_url: self.upload_url.clone(),
            task_id: self.task_id.clone(),
        }
    }

    /// Uploads the file body from a byte stream after rapid-upload failed.
    pub async fn upload_stream<S, E>(self, stream: S) -> Result<UploadComplete>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
        E: Into<crate::QuarkPanError>,
    {
        crate::upload::upload_stream(self, stream).await
    }

    /// Uploads from a resumed stream position using persisted part state.
    pub async fn upload_stream_resumable<S, E, F>(
        self,
        stream: S,
        state: UploadResumeState,
        on_part_uploaded: F,
    ) -> Result<UploadComplete>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Send + 'static,
        E: Into<crate::QuarkPanError>,
        F: FnMut(&UploadResumeState) -> Result<()> + Send + 'static,
    {
        crate::upload::upload_stream_resumable(self, stream, state, on_part_uploaded).await
    }
}

pub enum UploadPrepareResult {
    RapidUploaded { file_id: String },
    NeedUpload(UploadSession),
}

/// Final upload result after either rapid-upload or streaming upload.
pub struct UploadComplete {
    pub file_id: String,
    pub rapid_upload: bool,
}
