use std::cmp::min;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base64::{Engine as _, engine::general_purpose};
use dashmap::DashMap;
use futures_util::StreamExt;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderValue, RANGE};
use serde::Serialize;

use crate::error::{QuarkPanError, Result};
use crate::model::{
    AuthRequest, AuthResponse, CreateFolderRequest, CreateFolderResponse, DeleteFilesRequest,
    DeleteFilesResponse, DownloadInfo, EmptyData, FileDownloadUrlItem, FinishRequest,
    FinishResponse, GetFilesDownloadUrlsRequest, GetFilesDownloadUrlsResponse, ListFolderResponse,
    ListPage, RenameFileRequest, RenameFileResponse, Response, UpAuthAndCommitRequest,
    UpHashRequest, UpHashResponse, UpPartMethodRequest, UpPreRequest, UpPreResponse,
};

const ORIGIN: &str = "https://pan.quark.cn";
const REFERER: &str = "https://pan.quark.cn/";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/2.5.20 Chrome/100.0.4896.160 Electron/18.3.5.4-b478491100 Safari/537.36 Channel/pckk_other_ch";

#[derive(Clone)]
pub struct ApiConfig {
    pub api_base_url: String,
    pub cookie: Arc<DashMap<String, String>>,
}

#[derive(Clone)]
pub struct ApiClient {
    config: ApiConfig,
    client: reqwest::Client,
    download_client: reqwest::Client,
}

impl ApiClient {
    pub fn new(config: ApiConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("Origin", HeaderValue::from_static(ORIGIN));
        headers.insert("Referer", HeaderValue::from_static(REFERER));
        let pool_size: usize = min(num_cpus::get().saturating_mul(2), 16).max(3);
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .default_headers(headers.clone())
            .pool_idle_timeout(Duration::from_secs(50))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(pool_size)
            .timeout(Duration::from_secs(300))
            .build()?;
        let download_client = reqwest::Client::builder()
            .user_agent(UA)
            .default_headers(headers)
            .pool_idle_timeout(Duration::from_secs(50))
            .pool_max_idle_per_host(pool_size)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(300))
            .build()?;
        Ok(Self {
            config,
            client,
            download_client,
        })
    }

    async fn resolve_cookies(&self) -> String {
        self.config
            .cookie
            .iter()
            .map(|entry| format!("{}={}", entry.key(), entry.value()))
            .collect::<Vec<_>>()
            .join("; ")
    }

    async fn update_cookie_from_response(&self, res: &reqwest::Response) {
        if let Some(set_cookie) = res
            .headers()
            .get_all("set-cookie")
            .iter()
            .find_map(|v| v.to_str().ok())
        {
            if let Some(puus) = set_cookie
                .split(';')
                .find(|s| s.trim().starts_with("__puus="))
            {
                let new_puus = puus.trim().replace("__puus=", "");
                self.config.cookie.insert("__puus".to_string(), new_puus);
            }
        }
    }

    fn ensure_ok<T, U>(&self, response: Response<T, U>) -> Result<Response<T, U>> {
        if response.status != 200 {
            return Err(QuarkPanError::Api {
                status: response.code,
                message: response.message,
            });
        }
        Ok(response)
    }

    async fn get_json<U>(&self, url: String) -> Result<U>
    where
        U: serde::de::DeserializeOwned,
    {
        let cookie = self.resolve_cookies().await;
        let res = self.client.get(url).header("Cookie", cookie).send().await?;
        let res = res.error_for_status()?;
        self.update_cookie_from_response(&res).await;
        let text = res.text().await?;
        Ok(serde_json::from_str::<U>(&text)?)
    }

    async fn post_json<T, U>(&self, url: String, body: &T) -> Result<U>
    where
        T: Serialize + ?Sized,
        U: serde::de::DeserializeOwned,
    {
        let cookie = self.resolve_cookies().await;
        let res = self
            .client
            .post(url)
            .json(body)
            .header("Cookie", cookie)
            .send()
            .await?;
        let res = res.error_for_status()?;
        self.update_cookie_from_response(&res).await;
        let text = res.text().await?;
        Ok(serde_json::from_str::<U>(&text)?)
    }

    async fn post_xml<U>(&self, url: String, xml: String, headers: HeaderMap) -> Result<U>
    where
        U: serde::de::DeserializeOwned,
    {
        let cookie = self.resolve_cookies().await;
        let res = self
            .client
            .post(url)
            .headers(headers)
            .header("Cookie", cookie)
            .body(xml)
            .send()
            .await?;
        let res = res.error_for_status()?;
        self.update_cookie_from_response(&res).await;
        let text = res.text().await?;
        if text.trim().is_empty() {
            return Ok(serde_json::from_str("{}")?);
        }
        Ok(serde_json::from_str::<U>(&text)?)
    }

    pub async fn get_download_info(&self, fid: &str) -> Result<DownloadInfo> {
        let req = GetFilesDownloadUrlsRequest {
            fids: vec![fid.to_string()],
        };
        let res: GetFilesDownloadUrlsResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/download?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        let res = self.ensure_ok(res)?;
        res.data
            .into_iter()
            .next()
            .map(|item: FileDownloadUrlItem| DownloadInfo {
                fid: item.fid,
                download_url: item.download_url,
                md5: item.md5,
            })
            .ok_or_else(|| {
                QuarkPanError::invalid_argument(format!("download url not found for fid {fid}"))
            })
    }

    pub async fn get_download_url(&self, fid: &str) -> Result<String> {
        Ok(self.get_download_info(fid).await?.download_url)
    }

    pub async fn create_folder(&self, pdir_fid: &str, file_name: &str) -> Result<String> {
        let req = CreateFolderRequest {
            pdir_fid: pdir_fid.to_string(),
            file_name: file_name.to_string(),
            dir_path: String::new(),
            dir_init_lock: false,
        };
        let res: CreateFolderResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        Ok(self.ensure_ok(res)?.data.fid)
    }

    pub async fn rename(&self, fid: &str, file_name: &str) -> Result<()> {
        let req = RenameFileRequest {
            fid: fid.to_string(),
            file_name: file_name.to_string(),
        };
        let res: RenameFileResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/rename?pr=ucpro&fr=pc&uc_param_str=",
                    self.rename_api_base_url()
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)?;
        Ok(())
    }

    pub async fn delete<S>(&self, fids: &[S]) -> Result<()>
    where
        S: AsRef<str>,
    {
        if fids.is_empty() {
            return Err(QuarkPanError::invalid_argument(
                "at least one fid is required",
            ));
        }
        let req = DeleteFilesRequest {
            action_type: 2,
            exclude_fids: Vec::new(),
            filelist: fids.iter().map(|fid| fid.as_ref().to_string()).collect(),
        };
        let res: DeleteFilesResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/delete?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)?;
        Ok(())
    }

    pub async fn list_folder(&self, pdir_fid: &str, page: u32, size: u32) -> Result<ListPage> {
        let res: ListFolderResponse = self
            .get_json(
                format!(
                    "{}/1/clouddrive/file/sort?pr=ucpro&fr=pc&&pdir_fid={}&_page={}&_size={}&_fetch_total=1&_fetch_sub_dirs=0&_sort=file_type:asc,updated_at:desc,",
                    self.config.api_base_url, pdir_fid, page, size
                ),
            )
            .await?;
        let res = self.ensure_ok(res)?;
        Ok(ListPage {
            entries: res.data.list,
            page: res.metadata.page,
            size: res.metadata.count,
            total: res.metadata.total,
        })
    }

    pub async fn up_pre(
        &self,
        file_name: &str,
        size: u64,
        pdir_fid: &str,
    ) -> Result<UpPreResponse> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| QuarkPanError::invalid_argument(e.to_string()))?
            .as_millis() as u64;
        let req = UpPreRequest {
            file_name: file_name.to_string(),
            size,
            pdir_fid: pdir_fid.to_string(),
            format_type: get_format_type(file_name).to_string(),
            ccp_hash_update: true,
            l_created_at: now,
            l_updated_at: now,
            dir_name: String::new(),
            parallel_upload: false,
        };
        let res: UpPreResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/upload/pre?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)
    }

    pub async fn up_hash(&self, md5: &str, sha1: &str, task_id: &str) -> Result<UpHashResponse> {
        let req = UpHashRequest {
            md5: md5.to_string(),
            sha1: sha1.to_string(),
            task_id: task_id.to_string(),
        };
        let res: UpHashResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/update/hash?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)
    }

    pub async fn auth(
        &self,
        auth_info: &str,
        auth_meta: &str,
        task_id: &str,
    ) -> Result<AuthResponse> {
        let req = AuthRequest {
            auth_info: auth_info.to_string(),
            auth_meta: auth_meta.to_string(),
            task_id: task_id.to_string(),
        };
        let res: AuthResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/upload/auth?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)
    }

    pub async fn finish(&self, obj_key: &str, task_id: &str) -> Result<FinishResponse> {
        let req = FinishRequest {
            obj_key: obj_key.to_string(),
            task_id: task_id.to_string(),
        };
        let res: FinishResponse = self
            .post_json(
                format!(
                    "{}/1/clouddrive/file/upload/finish?pr=ucpro&fr=pc",
                    self.config.api_base_url
                ),
                &req,
            )
            .await?;
        self.ensure_ok(res)
    }

    pub fn up_part_auth_meta(
        &self,
        mime_type: &str,
        utc_time: &str,
        bucket: &str,
        obj_key: &str,
        part_number: u32,
        upload_id: &str,
    ) -> String {
        format!(
            "PUT\n\n{mime_type}\n{utc_time}\nx-oss-date:{utc_time}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{bucket}/{obj_key}?partNumber={part_number}&uploadId={upload_id}"
        )
    }

    pub async fn up_part(&self, req: UpPartMethodRequest) -> Result<String> {
        let oss_url = format!(
            "https://{}.{}//{}?partNumber={}&uploadId={}",
            req.bucket, req.upload_url, req.obj_key, req.part_number, req.upload_id
        );
        let res = self
            .client
            .put(oss_url)
            .header("Authorization", req.auth_key)
            .header("Content-Type", req.mime_type)
            .header("x-oss-date", req.utc_time)
            .header(
                "x-oss-user-agent",
                "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit",
            )
            .header("Referer", REFERER)
            .body(req.part_bytes)
            .send()
            .await?;
        let res = res.error_for_status()?;
        let etag = res
            .headers()
            .get("Etag")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| QuarkPanError::invalid_argument("missing Etag in upload response"))?;
        Ok(etag.to_string())
    }

    pub async fn up_auth_and_commit(&self, req: UpAuthAndCommitRequest) -> Result<()> {
        let xml_body = build_complete_upload_xml(&req.md5s);
        let digest = md5::compute(xml_body.as_bytes());
        let content_md5 = general_purpose::STANDARD.encode(digest.0);
        let callback_base64 = general_purpose::STANDARD.encode(serde_json::to_vec(&req.callback)?);
        let time_str = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let auth_meta = format!(
            "POST\n{}\napplication/xml\n{}\nx-oss-callback:{}\nx-oss-date:{}\nx-oss-user-agent:aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit\n/{}/{}?uploadId={}",
            content_md5,
            time_str,
            callback_base64,
            time_str,
            req.bucket,
            req.obj_key,
            req.upload_id
        );
        let auth_res = self.auth(&req.auth_info, &auth_meta, &req.task_id).await?;
        let auth_key = auth_res.data.auth_key;
        let commit_url = format!(
            "https://{}.{}/{}?uploadId={}",
            req.bucket, req.upload_url, req.obj_key, req.upload_id
        );
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_str(&auth_key)?);
        headers.insert("Content-MD5", HeaderValue::from_str(&content_md5)?);
        headers.insert("Content-Type", HeaderValue::from_static("application/xml"));
        headers.insert("x-oss-callback", HeaderValue::from_str(&callback_base64)?);
        headers.insert("x-oss-date", HeaderValue::from_str(&time_str)?);
        headers.insert(
            "x-oss-user-agent",
            HeaderValue::from_static(
                "aliyun-sdk-js/6.6.1 Chrome 98.0.4758.80 on Windows 10 64-bit",
            ),
        );
        headers.insert("Referer", HeaderValue::from_static(REFERER));
        let _: EmptyData = self
            .post_xml(commit_url, xml_body, headers)
            .await
            .or_else(|err| match err {
                QuarkPanError::Serde(_) => Ok(EmptyData {}),
                _ => Err(err),
            })?;
        Ok(())
    }

    pub async fn download_stream(
        &self,
        fid: &str,
        start_offset: Option<u64>,
    ) -> Result<crate::model::BoxByteStream> {
        let url = self.get_download_url(fid).await?;
        let cookie = self.resolve_cookies().await;
        let mut req = self.download_client.get(url).header("Cookie", cookie);
        if let Some(start_offset) = start_offset {
            req = req.header(RANGE, format!("bytes={start_offset}-"));
        }
        let res = req.send().await?;
        let res = res.error_for_status()?;
        if start_offset.is_some() && res.status() != StatusCode::PARTIAL_CONTENT {
            return Err(QuarkPanError::invalid_argument(
                "server did not honor range request for resume download",
            ));
        }
        let stream = res
            .bytes_stream()
            .map(|chunk| chunk.map_err(QuarkPanError::from));
        Ok(Box::pin(stream))
    }
}

impl ApiClient {
    fn rename_api_base_url(&self) -> String {
        self.config
            .api_base_url
            .replace("https://drive.quark.cn", "https://drive-pc.quark.cn")
            .replace("http://drive.quark.cn", "http://drive-pc.quark.cn")
    }
}

fn build_complete_upload_xml(md5s: &[String]) -> String {
    let mut xml_body =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CompleteMultipartUpload>\n");
    for (i, md5) in md5s.iter().enumerate() {
        xml_body.push_str(&format!(
            "<Part>\n<PartNumber>{}</PartNumber>\n<ETag>{}</ETag>\n</Part>\n",
            i + 1,
            md5
        ));
    }
    xml_body.push_str("</CompleteMultipartUpload>");
    xml_body
}

fn get_format_type(file_name: &str) -> &'static str {
    mime_guess::from_path(file_name)
        .first_raw()
        .unwrap_or("application/octet-stream")
}
