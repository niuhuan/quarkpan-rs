use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use futures_util::Stream;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use libquarkpan::{
    ListPage, ProgressStream, QuarkPan, QuarkPanError, TransferControl, TransferProgress,
    UploadPrepareResult, UploadResume, UploadResumeState,
};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use sha1::Digest;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

#[derive(Parser, Debug)]
#[command(name = "quarkpan", version, about)]
struct Cli {
    #[arg(long, env = "QUARK_COOKIE")]
    cookie: Option<String>,
    #[arg(long)]
    cookie_file: Option<PathBuf>,
    #[arg(long, default_value = "https://drive.quark.cn")]
    api_base_url: String,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(long)]
    no_progress: bool,
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    color: ColorMode,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand, Debug)]
enum Commands {
    List(ListArgs),
    Download(DownloadArgs),
    Folder(FolderArgs),
    Upload(UploadArgs),
}

#[derive(Args, Debug)]
struct ListArgs {
    #[arg(long, default_value = "0")]
    folder_id: String,
    #[arg(long, default_value_t = 1)]
    page: u32,
    #[arg(long, default_value_t = 100)]
    size: u32,
    #[arg(long)]
    all: bool,
    #[arg(long)]
    more: bool,
    #[arg(long)]
    long: bool,
    #[arg(long)]
    raw_time: bool,
}

#[derive(Args, Debug)]
struct DownloadArgs {
    #[arg(long)]
    file_id: String,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    stdout: bool,
    #[arg(long)]
    overwrite: bool,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
}

#[derive(Args, Debug)]
struct FolderArgs {
    #[command(subcommand)]
    command: FolderCommand,
}

#[derive(Subcommand, Debug)]
enum FolderCommand {
    Create(FolderCreateArgs),
}

#[derive(Args, Debug)]
struct FolderCreateArgs {
    #[arg(long, default_value = "0")]
    parent_folder: String,
    #[arg(long)]
    name: String,
}

#[derive(Args, Debug)]
struct UploadArgs {
    #[arg(long, default_value = "0")]
    parent_folder: String,
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
}

#[derive(Clone, Copy)]
struct OutputFlags {
    json: bool,
    quiet: bool,
    no_progress: bool,
    color: bool,
}

#[derive(Serialize)]
struct OutputMessage<T: Serialize> {
    ok: bool,
    data: T,
}

#[derive(Serialize)]
struct FolderCreateOutput {
    folder_id: String,
}

#[derive(Serialize)]
struct UploadDoneOutput {
    file_id: String,
    rapid_upload: bool,
}

#[derive(Serialize)]
struct HashOutput {
    name: String,
    size: u64,
    md5: String,
    sha1: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DownloadTask {
    kind: String,
    file_id: String,
    output_path: String,
    md5: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UploadTask {
    kind: String,
    file_path: String,
    file_name: String,
    parent_folder: String,
    size: u64,
    md5: String,
    sha1: String,
    resume: UploadResume,
    state: UploadResumeState,
}

#[tokio::main]
async fn main() {
    let code = match run().await {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    };
    std::process::exit(code);
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    validate_cli(&cli)?;
    let cookie = load_cookie(&cli).await?;
    let flags = OutputFlags {
        json: cli.json,
        quiet: cli.quiet,
        no_progress: cli.no_progress,
        color: resolve_color(cli.color),
    };
    let quark_pan = QuarkPan::builder()
        .cookie(cookie)
        .api_base_url(cli.api_base_url)
        .prepare()?;

    match cli.command {
        Commands::List(args) => handle_list(flags, &quark_pan, args).await?,
        Commands::Download(args) => handle_download(flags, &quark_pan, args).await?,
        Commands::Folder(args) => handle_folder(flags, &quark_pan, args).await?,
        Commands::Upload(args) => handle_upload(flags, &quark_pan, args).await?,
    }
    Ok(())
}

fn validate_cli(cli: &Cli) -> Result<(), QuarkPanError> {
    if cli.cookie.is_some() && cli.cookie_file.is_some() {
        return Err(QuarkPanError::invalid_argument(
            "--cookie and --cookie-file cannot be used together",
        ));
    }
    Ok(())
}

async fn load_cookie(cli: &Cli) -> Result<String, QuarkPanError> {
    if let Some(cookie) = &cli.cookie {
        return Ok(cookie.clone());
    }
    if let Some(cookie_file) = &cli.cookie_file {
        let cookie = tokio::fs::read_to_string(cookie_file).await?;
        return Ok(cookie.trim().to_string());
    }
    Err(QuarkPanError::missing_field("cookie"))
}

async fn handle_list(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ListArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if flags.json && args.more {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "--json cannot be combined with --more",
        )));
    }
    if args.all {
        let mut page_no = args.page;
        let mut entries = Vec::new();
        loop {
            let page = quark_pan
                .list()
                .folder_id(args.folder_id.clone())
                .page(page_no)
                .size(args.size)
                .prepare()?
                .request()
                .await?;
            let count = page.entries.len();
            let total = page.total;
            entries.extend(page.entries);
            if count < args.size as usize {
                let aggregated = ListPage {
                    entries,
                    page: page_no,
                    size: args.size,
                    total,
                };
                return print_list_output(flags, &aggregated, args.long, args.raw_time);
            }
            page_no += 1;
        }
    }
    if args.more {
        return handle_list_more(flags, quark_pan, args).await;
    }
    let page = quark_pan
        .list()
        .folder_id(args.folder_id)
        .page(args.page)
        .size(args.size)
        .prepare()?
        .request()
        .await?;
    print_list_output(flags, &page, args.long, args.raw_time)
}

async fn handle_list_more(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ListArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut page_no = args.page;
    let stdin = std::io::stdin();
    loop {
        let page = quark_pan
            .list()
            .folder_id(args.folder_id.clone())
            .page(page_no)
            .size(args.size)
            .prepare()?
            .request()
            .await?;
        print_list_output(flags, &page, args.long, args.raw_time)?;
        if page.entries.len() < args.size as usize {
            break;
        }
        print!("-- More -- page {} | Enter next page | q quit: ", page_no);
        std::io::stdout().flush()?;
        let mut line = String::new();
        stdin.read_line(&mut line)?;
        if line.trim().eq_ignore_ascii_case("q") {
            break;
        }
        page_no += 1;
    }
    Ok(())
}

fn print_list_output(
    flags: OutputFlags,
    page: &ListPage,
    long: bool,
    raw_time: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if flags.json {
        println!("{}", serde_json::to_string_pretty(page)?);
        return Ok(());
    }
    if !flags.quiet {
        println!(
            "page={} shown={} total={}{}",
            page.page,
            page.entries.len(),
            page.total,
            if long { " (long)" } else { "" }
        );
    }
    if long {
        println!(
            "{}",
            format_header(
                flags,
                &format!(
                    "{:<4} {:>12} {:<16} {} {}",
                    "TYPE", "SIZE", "UPDATED", "FID", "NAME"
                )
            )
        );
    } else {
        println!(
            "{}",
            format_header(
                flags,
                &format!("{:<4} {:>12} {} {}", "TYPE", "SIZE", "FID", "NAME")
            )
        );
    }
    for entry in &page.entries {
        let ty = if entry.dir { "DIR" } else { "FILE" };
        let size = if entry.dir {
            "-".to_string()
        } else {
            entry.size.to_string()
        };
        if long {
            println!(
                "{:<4} {:>12} {:<16} {} {}",
                ty,
                size,
                format_time(entry.updated_at, raw_time),
                entry.fid,
                entry.file_name
            );
        } else {
            println!("{:<4} {:>12} {} {}", ty, size, entry.fid, entry.file_name);
        }
    }
    Ok(())
}

async fn handle_download(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: DownloadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.output.is_some() == args.stdout {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "exactly one of --output or --stdout is required",
        )));
    }
    let request = quark_pan
        .download()
        .file_id(args.file_id.clone())
        .prepare()?;
    if args.stdout {
        let mut stream = request.stream().await?;
        let mut stdout = tokio::io::stdout();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            stdout.write_all(chunk.as_ref()).await?;
        }
        stdout.flush().await?;
        return Ok(());
    }

    let info = request.info().await?;
    let output = args.output.expect("checked above");
    let task_path = download_task_path(&output);
    if has_same_download_target(&output, info.md5.as_deref()).await? {
        cleanup_download_artifacts(&output, &task_path).await?;
        if !flags.quiet {
            eprintln!("download skipped: local file already matches remote md5");
        }
        return Ok(());
    }

    let existing_task = read_download_task(&task_path).await?;
    if let Some(task) = &existing_task {
        let same_target =
            task.file_id == args.file_id && task.output_path == output.to_string_lossy();
        if !same_target {
            cleanup_download_artifacts(&output, &task_path).await?;
        }
    }

    if output.exists() && !args.overwrite && !args.continue_download {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "output already exists: {} (use --overwrite or --continue)",
            output.display()
        ))));
    }

    if args.overwrite && output.exists() {
        cleanup_download_artifacts(&output, &task_path).await?;
    }

    // The task file records the remote identity so `--continue` can detect
    // whether the local partial file still belongs to the same download job.
    let task = DownloadTask {
        kind: "download".to_string(),
        file_id: args.file_id.clone(),
        output_path: output.to_string_lossy().to_string(),
        md5: info.md5.clone(),
    };
    write_json_file(&task_path, &task).await?;

    let resume_from = if args.continue_download && output.exists() {
        tokio::fs::metadata(&output).await?.len()
    } else {
        0
    };
    let mut builder = quark_pan.download().file_id(args.file_id.clone());
    if resume_from > 0 {
        builder = builder.start_offset(resume_from);
    }
    let mut stream = builder.prepare()?.stream().await?;
    let mut file = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output)
            .await?
    } else {
        tokio::fs::File::create(&output).await?
    };

    let result = if flags.no_progress || flags.quiet {
        write_stream_to_file(&mut stream, &mut file).await
    } else {
        let control = TransferControl::new(None);
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), "downloaded");
        let mut stream = ProgressStream::new(stream, control);
        let result = write_stream_to_file(&mut stream, &mut file).await;
        eprintln!();
        result
    };

    match result {
        Ok(()) => {
            file.flush().await?;
            if let Some(md5) = info.md5.as_deref() {
                let local = md5_file(&output).await?;
                if !local.eq_ignore_ascii_case(md5) {
                    return Err(Box::new(QuarkPanError::invalid_argument(format!(
                        "download completed but md5 mismatch: local={}, remote={}",
                        local, md5
                    ))));
                }
            }
            remove_if_exists(&task_path).await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

async fn handle_folder(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: FolderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        FolderCommand::Create(args) => {
            let folder_id = quark_pan
                .create_folder()
                .parent_folder(args.parent_folder)
                .name(args.name)
                .prepare()?
                .request()
                .await?;
            print_output(flags, &FolderCreateOutput { folder_id })?;
        }
    }
    Ok(())
}

async fn handle_upload(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let task_path = upload_task_path(&args.file);
    if args.r#continue {
        return resume_upload(flags, quark_pan, args, task_path).await;
    }

    let local = hash_file(&args.file, args.name.as_deref()).await?;
    let prepared = quark_pan
        .upload()
        .parent_folder(args.parent_folder.clone())
        .name(local.name.clone())
        .size(local.size)
        .md5(local.md5.clone())
        .sha1(local.sha1.clone())
        .prepare()
        .await?;

    match prepared {
        UploadPrepareResult::RapidUploaded { file_id } => {
            remove_if_exists(&task_path).await?;
            print_output(
                flags,
                &UploadDoneOutput {
                    file_id,
                    rapid_upload: true,
                },
            )?;
        }
        UploadPrepareResult::NeedUpload(session) => {
            let upload_task = UploadTask {
                kind: "upload".to_string(),
                file_path: args.file.to_string_lossy().to_string(),
                file_name: local.name.clone(),
                parent_folder: args.parent_folder,
                size: local.size,
                md5: local.md5,
                sha1: local.sha1,
                resume: session.to_resume(),
                state: UploadResumeState {
                    next_part_number: 1,
                    part_etags: Vec::new(),
                },
            };
            write_json_file(&task_path, &upload_task).await?;
            let completed = upload_file_with_task(
                flags,
                quark_pan,
                &args.file,
                upload_task.clone(),
                task_path.as_path(),
            )
            .await?;
            remove_if_exists(&task_path).await?;
            print_output(
                flags,
                &UploadDoneOutput {
                    file_id: completed.file_id,
                    rapid_upload: completed.rapid_upload,
                },
            )?;
        }
    }
    Ok(())
}

async fn resume_upload(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadArgs,
    task_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(task) = read_upload_task(&task_path).await? else {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "upload task file not found: {}",
            task_path.display()
        ))));
    };
    let file_path = PathBuf::from(&task.file_path);
    if file_path != args.file {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "--file does not match task file",
        )));
    }
    let local = hash_file(&args.file, Some(&task.file_name)).await?;
    if local.size != task.size || local.md5 != task.md5 || local.sha1 != task.sha1 {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "local file size/md5/sha1 does not match upload task",
        )));
    }
    let completed =
        upload_file_with_task(flags, quark_pan, &args.file, task, task_path.as_path()).await?;
    remove_if_exists(&task_path).await?;
    print_output(
        flags,
        &UploadDoneOutput {
            file_id: completed.file_id,
            rapid_upload: completed.rapid_upload,
        },
    )?;
    Ok(())
}

async fn upload_file_with_task(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    file_path: &Path,
    mut task: UploadTask,
    task_path: &Path,
) -> Result<libquarkpan::UploadComplete, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(file_path).await?;
    let start_part = task.state.next_part_number.max(1);
    // Resume starts at the first not-yet-committed part boundary. The task file
    // persists both the next part number and all committed ETags for final commit.
    let seek_to = ((start_part - 1) as u64) * task.resume.part_size;
    if seek_to > 0 {
        file.seek(std::io::SeekFrom::Start(seek_to)).await?;
    }
    let stream = ReaderStream::new(file);
    let session = quark_pan.upload().resume(task.resume.clone());
    let total_remaining = task.size.saturating_sub(seek_to);
    let state = task.state.clone();
    let task_path = task_path.to_path_buf();

    let on_part_uploaded = move |state: &UploadResumeState| -> libquarkpan::Result<()> {
        task.state = state.clone();
        let data = serde_json::to_vec_pretty(&task)?;
        std::fs::write(&task_path, data)?;
        Ok(())
    };

    if flags.no_progress || flags.quiet {
        Ok(session
            .upload_stream_resumable(stream, state, on_part_uploaded)
            .await?)
    } else {
        let control = TransferControl::new(Some(total_remaining));
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), "uploaded");
        let stream = ProgressStream::new(stream, control);
        let completed = session
            .upload_stream_resumable(stream, state, on_part_uploaded)
            .await?;
        eprintln!();
        Ok(completed)
    }
}

async fn write_stream_to_file<S>(
    stream: &mut S,
    file: &mut tokio::fs::File,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Stream<Item = Result<bytes::Bytes, QuarkPanError>> + Unpin,
{
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(chunk.as_ref()).await?;
    }
    Ok(())
}

async fn read_download_task(
    path: &Path,
) -> Result<Option<DownloadTask>, Box<dyn std::error::Error>> {
    read_json_file(path).await
}

async fn read_upload_task(path: &Path) -> Result<Option<UploadTask>, Box<dyn std::error::Error>> {
    read_json_file(path).await
}

async fn read_json_file<T>(path: &Path) -> Result<Option<T>, Box<dyn std::error::Error>>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(None);
    }
    let data = tokio::fs::read(path).await?;
    Ok(Some(serde_json::from_slice(&data)?))
}

async fn write_json_file<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = serde_json::to_vec_pretty(value)?;
    tokio::fs::write(path, data).await?;
    Ok(())
}

fn download_task_path(output: &Path) -> PathBuf {
    task_path_for(output)
}

fn upload_task_path(file: &Path) -> PathBuf {
    task_path_for(file)
}

fn task_path_for(path: &Path) -> PathBuf {
    let base = path.as_os_str().to_string_lossy().to_string();
    PathBuf::from(format!("{base}.quark.task"))
}

async fn cleanup_download_artifacts(
    output: &Path,
    task_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    remove_if_exists(output).await?;
    remove_if_exists(task_path).await?;
    Ok(())
}

async fn remove_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn has_same_download_target(
    output: &Path,
    remote_md5: Option<&str>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !output.exists() {
        return Ok(false);
    }
    let Some(remote_md5) = remote_md5 else {
        return Ok(false);
    };
    let local_md5 = md5_file(output).await?;
    Ok(local_md5.eq_ignore_ascii_case(remote_md5))
}

async fn hash_file(
    path: &Path,
    name: Option<&str>,
) -> Result<HashOutput, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut md5_ctx = md5::Context::new();
    let mut sha1_ctx = sha1::Sha1::new();
    let mut size = 0_u64;
    let mut buf = vec![0_u8; 1024 * 1024];

    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        md5_ctx.consume(&buf[..read]);
        sha1_ctx.update(&buf[..read]);
    }

    let name = match name {
        Some(name) => name.to_string(),
        None => path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid file name"))?
            .to_string(),
    };
    Ok(HashOutput {
        name,
        size,
        md5: format!("{:x}", md5_ctx.compute()),
        sha1: format!("{:x}", sha1_ctx.finalize()),
    })
}

async fn md5_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut md5_ctx = md5::Context::new();
    let mut buf = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        md5_ctx.consume(&buf[..read]);
    }
    Ok(format!("{:x}", md5_ctx.compute()))
}

fn print_output<T: Serialize>(
    flags: OutputFlags,
    data: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if flags.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&OutputMessage { ok: true, data })?
        );
    } else {
        let rendered = serde_json::to_string_pretty(data)?;
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    }
    Ok(())
}

fn spawn_ctrl_c_cancel(control: TransferControl) {
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        control.cancel();
    });
}

fn spawn_progress_printer(control: TransferControl, label: &'static str) {
    let progress_bar = create_progress_bar(label, control.snapshot().total);
    tokio::spawn(async move {
        let mut rx = control.subscribe();
        while rx.changed().await.is_ok() {
            let progress = *rx.borrow();
            update_progress_bar(&progress_bar, label, progress);
        }
        progress_bar.finish_and_clear();
    });
}

fn create_progress_bar(label: &str, total: Option<u64>) -> ProgressBar {
    let bar = match total {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };
    let style = match total {
        Some(_) => ProgressStyle::with_template(
            "{spinner:.green} {msg:<10} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        )
        .unwrap()
        .progress_chars("=> "),
        None => ProgressStyle::with_template("{spinner:.green} {msg:<10} {bytes} ({bytes_per_sec})")
            .unwrap(),
    };
    bar.set_style(style);
    bar.set_message(label.to_string());
    bar
}

fn update_progress_bar(progress_bar: &ProgressBar, label: &str, progress: TransferProgress) {
    progress_bar.set_message(label.to_string());
    progress_bar.set_position(progress.transferred);
    if progress.total.is_none() {
        progress_bar.tick();
    }
}

fn resolve_color(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal() || std::io::stderr().is_terminal(),
    }
}

fn format_header(flags: OutputFlags, text: &str) -> String {
    if flags.color {
        text.bold().cyan().to_string()
    } else {
        text.to_string()
    }
}

fn format_time(ts_millis: u64, raw: bool) -> String {
    if raw {
        return ts_millis.to_string();
    }
    let secs = (ts_millis / 1000) as i64;
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}
