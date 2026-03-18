# libquarkpan

`libquarkpan` 是夸克网盘的异步 Rust 客户端库。

它的定位是“核心能力库”，重点提供可组合的请求对象和流式上传下载接口，而不是直接替上层应用管理所有文件逻辑。

## 功能概览

当前版本支持：

- 用 Cookie 构造 `QuarkPan` 客户端
- 列出指定 `pdir_fid` 下的内容
- 在指定 `pdir_fid` 下创建目录
- 重命名指定文件或目录项
- 删除一个或多个文件或目录项
- 按 `fid` 获取下载信息和下载流
- 上传预检
- 快传判断
- 分片上传
- 上传和下载的进度统计
- 传输取消
- 上传分片级重试与完成阶段重试
- 导出 `UploadResume` / `UploadResumeState` 供上层做断点续传

## 设计边界

当前库刻意保留了以下边界：

- 根目录默认使用 `pdir_fid = "0"`
- 下载当前只支持按 `fid`
- 上传和下载首先暴露流接口，不直接内置“从文件路径到写盘完成”的一体化 API
- 路径解析、目录缓存、任务文件持久化由上层应用或 CLI 负责
- 文件夹级同步和任务编排主要由 CLI 或上层应用负责

这种设计的目的是避免把上层业务策略耦合进底层库。

## 快速开始

### 创建客户端

```rust
use libquarkpan::QuarkPan;

let quark_pan = QuarkPan::builder()
    .cookie("k1=v1; k2=v2")
    .prepare()?;
```

### 列出目录

```rust
let page = quark_pan
    .list()
    .pdir_fid("0")
    .page(1)
    .size(100)
    .prepare()?
    .request()
    .await?;
```

### 创建目录

```rust
let fid = quark_pan
    .create_folder()
    .pdir_fid("0")
    .file_name("我的文档")
    .prepare()?
    .request()
    .await?;
```

### 删除文件或目录项

```rust
quark_pan.delete(&["fid1", "fid2"]).await?;
```

### 重命名文件或目录项

```rust
quark_pan
    .rename()
    .fid("fid")
    .file_name("新的名字")
    .prepare()?
    .request()
    .await?;
```

### 下载文件

```rust
let request = quark_pan
    .download()
    .fid("your-file-id")
    .prepare()?;

let info = request.info().await?;
let mut stream = request.stream().await?;
```

如果需要续传，可以在构造下载请求时设置 `start_offset(...)`。

### 上传预检

上传前需要先知道文件的：

- `size`
- `md5`
- `sha1`

```rust
let prepared = quark_pan
    .upload()
    .pdir_fid("0")
    .file_name("1.xlsx")
    .size(123)
    .md5("...")
    .sha1("...")
    .prepare()
    .await?;
```

返回值分两类：

- `UploadPrepareResult::RapidUploaded`
  代表云端已命中快传
- `UploadPrepareResult::NeedUpload(session)`
  代表需要继续上传文件流

### 继续上传文件流

```rust
use tokio_util::io::ReaderStream;

let file = tokio::fs::File::open("./1.xlsx").await?;
let stream = ReaderStream::new(file);

let completed = session.upload_stream(stream).await?;
```

如果你希望自己实现续传，可以持久化：

- `session.to_resume()`
- `UploadResumeState`

然后使用 `upload_stream_resumable(...)` 从某个分片位置继续。

## 错误处理

库统一使用 `thiserror` 风格的 `QuarkPanError`。

常见错误类型包括：

- 参数缺失
- 参数不合法
- HTTP 请求失败
- 远端 API 返回错误
- JSON 解析失败
- IO 错误
- 用户取消传输

## 取消与恢复

如果上层把下载流或上传流与 `TransferControl` 结合使用：

- 调用 `cancel()` 后传输会返回 `QuarkPanError::Cancelled`
- 库不会自动清理任务文件
- 是否恢复、如何恢复由上层自行决定

## 适合放在 examples 或上层应用中的逻辑

以下逻辑目前更适合放在 examples、CLI 或你的应用层：

- 从本地文件实时计算哈希
- 将下载流直接写入文件
- `.quark.task` 的创建、更新和清理
- CLI 进度条展示
- 基于路径的多级目录解析

## License

`GPL-3.0-only`
