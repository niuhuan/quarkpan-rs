use libquarkpan::{ProgressStream, QuarkPan, TransferControl, UploadPrepareResult};
use sha1::Digest;
use tokio_util::io::ReaderStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cookie = std::env::var("QUARK_COOKIE")?;
    let file_path = std::env::args().nth(1).expect("missing file path");
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .and_then(|v| v.to_str())
        .expect("invalid file name")
        .to_string();
    let bytes = tokio::fs::read(&file_path).await?;
    let size = bytes.len() as u64;
    let md5 = format!("{:x}", md5::compute(&bytes));
    let mut sha1_ctx = sha1::Sha1::new();
    sha1_ctx.update(&bytes);
    let sha1 = format!("{:x}", sha1_ctx.finalize());

    let quark_pan = QuarkPan::builder().cookie(cookie).prepare()?;
    let prepared = quark_pan
        .upload()
        .pdir_fid("0")
        .file_name(file_name)
        .size(size)
        .md5(md5)
        .sha1(sha1)
        .prepare()
        .await?;

    match prepared {
        UploadPrepareResult::RapidUploaded { fid } => {
            println!("rapid uploaded: {fid}");
        }
        UploadPrepareResult::NeedUpload(session) => {
            let fid = session.fid().to_string();
            let file = tokio::fs::File::open(&file_path).await?;
            let control = TransferControl::new(Some(size));
            let mut progress_rx = control.subscribe();
            tokio::spawn(async move {
                while progress_rx.changed().await.is_ok() {
                    let progress = *progress_rx.borrow();
                    eprintln!(
                        "upload progress: {}/{}",
                        progress.transferred,
                        progress.total.unwrap_or(0)
                    );
                }
            });
            let stream = ProgressStream::new(ReaderStream::new(file), control);
            let completed = session.upload_stream(stream).await?;
            println!(
                "uploaded: {}, rapid={}",
                completed.fid, completed.rapid_upload
            );
            let mut download = quark_pan.download().fid(fid).prepare()?.stream().await?;
            while let Some(chunk) = futures_util::StreamExt::next(&mut download).await {
                let chunk = chunk?;
                println!("downloaded chunk: {}", chunk.len());
                break;
            }
        }
    }

    Ok(())
}
