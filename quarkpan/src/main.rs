use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::ProjectDirs;
use futures_util::Stream;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use libquarkpan::{
    ListPage, ProgressStream, QuarkEntry, QuarkPan, QuarkPanError, TransferControl,
    TransferProgress, UploadPrepareResult, UploadResume, UploadResumeState,
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
    #[arg(long)]
    config_file: Option<PathBuf>,
    #[arg(long)]
    api_base_url: Option<String>,
    #[arg(long)]
    quiet: bool,
    #[arg(long)]
    no_progress: bool,
    #[arg(long, value_enum)]
    color: Option<ColorMode>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth(AuthArgs),
    List(ListArgs),
    Download(DownloadArgs),
    DownloadDir(DownloadDirArgs),
    Folder(FolderArgs),
    Rename(RenameArgs),
    Upload(UploadArgs),
    UploadDir(UploadDirArgs),
}

#[derive(Args, Debug)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Subcommand, Debug)]
enum AuthCommand {
    SetCookie(SetCookieArgs),
    ImportCookie(ImportCookieArgs),
    ClearCookie,
    ShowSource,
}

#[derive(Args, Debug)]
struct SetCookieArgs {
    #[arg(long)]
    cookie: Option<String>,
    #[arg(long, conflicts_with_all = ["cookie", "from_nano", "from_vi"])]
    from_stdin: bool,
    #[arg(long, conflicts_with_all = ["cookie", "from_stdin", "from_vi"])]
    from_nano: bool,
    #[arg(long, conflicts_with_all = ["cookie", "from_stdin", "from_nano"])]
    from_vi: bool,
}

#[derive(Args, Debug)]
struct ImportCookieArgs {
    #[arg(long)]
    from_file: PathBuf,
}

#[derive(Args, Debug, Clone)]
struct ListArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
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

#[derive(Args, Debug, Clone)]
struct DownloadArgs {
    #[arg(long)]
    fid: String,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    stdout: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
    #[arg(long, default_value_t = 5)]
    retry: u32,
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
}

#[derive(Args, Debug, Clone)]
struct DownloadDirArgs {
    #[arg(long)]
    pdir_fid: String,
    #[arg(long)]
    output: PathBuf,
    #[arg(long = "continue", short = 'c')]
    continue_download: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
    #[arg(long, default_value_t = 5)]
    retry: u32,
    #[arg(long, default_value_t = 2)]
    retry_delay: u64,
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
    pdir_fid: String,
    #[arg(long)]
    file_name: String,
}

#[derive(Args, Debug, Clone)]
struct RenameArgs {
    #[arg(long)]
    fid: String,
    #[arg(long)]
    file_name: String,
}

#[derive(Args, Debug, Clone)]
struct UploadArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    file_name: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
}

#[derive(Args, Debug, Clone)]
struct UploadDirArgs {
    #[arg(long, default_value = "0")]
    pdir_fid: String,
    #[arg(long)]
    dir: PathBuf,
    #[arg(long)]
    file_name: Option<String>,
    #[arg(long, short = 'c')]
    r#continue: bool,
    #[arg(long, short = 'o')]
    overwrite: bool,
}

#[derive(Clone, Copy)]
struct OutputFlags {
    quiet: bool,
    no_progress: bool,
    color: bool,
    interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppConfig {
    api_base_url: Option<String>,
    color: Option<ColorMode>,
}

#[derive(Debug, Clone)]
struct AppPaths {
    config_dir: PathBuf,
    config_file: PathBuf,
    cookie_file: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct FolderCreateOutput {
    fid: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UploadDoneOutput {
    fid: String,
    rapid_upload: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct RenameOutput {
    fid: String,
    file_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthSourceOutput {
    source: String,
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct HashOutput {
    name: String,
    size: u64,
    md5: String,
    sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadTask {
    kind: String,
    fid: String,
    output_path: String,
    md5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadTask {
    kind: String,
    file_path: String,
    file_name: String,
    pdir_fid: String,
    size: u64,
    md5: String,
    sha1: String,
    resume: UploadResume,
    state: UploadResumeState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DirEntryStatus {
    Pending,
    Running,
    Done,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadDirEntryTask {
    relative_path: String,
    fid: String,
    md5: Option<String>,
    status: DirEntryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadDirTask {
    kind: String,
    pdir_fid: String,
    output_dir: String,
    entries: Vec<DownloadDirEntryTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadDirEntryTask {
    relative_path: String,
    status: DirEntryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UploadDirTask {
    kind: String,
    source_dir: String,
    pdir_fid: String,
    target_file_name: String,
    root_fid: String,
    entries: Vec<UploadDirEntryTask>,
}

#[derive(Debug, Clone)]
struct RemoteFileItem {
    relative_path: PathBuf,
    fid: String,
}

#[derive(Debug, Clone)]
struct LocalFileItem {
    relative_path: PathBuf,
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

    let paths = app_paths(cli.config_file.clone())?;
    let config = load_config(&paths).await?;
    let flags = OutputFlags {
        quiet: cli.quiet,
        no_progress: cli.no_progress,
        color: resolve_color(cli.color.or(config.color).unwrap_or(ColorMode::Auto)),
        interactive: std::io::stderr().is_terminal(),
    };

    if let Commands::Auth(args) = cli.command {
        return handle_auth(flags, &paths, args).await;
    }

    let cookie = load_cookie(&cli, &paths).await?;
    let quark_pan = QuarkPan::builder()
        .cookie(cookie)
        .api_base_url(
            cli.api_base_url
                .or(config.api_base_url)
                .unwrap_or_else(|| "https://drive.quark.cn".to_string()),
        )
        .prepare()?;

    match cli.command {
        Commands::Auth(_) => unreachable!(),
        Commands::List(args) => handle_list(flags, &quark_pan, args).await?,
        Commands::Download(args) => handle_download(flags, &quark_pan, args).await?,
        Commands::DownloadDir(args) => handle_download_dir(flags, &quark_pan, args).await?,
        Commands::Folder(args) => handle_folder(flags, &quark_pan, args).await?,
        Commands::Rename(args) => handle_rename(flags, &quark_pan, args).await?,
        Commands::Upload(args) => handle_upload(flags, &quark_pan, args).await?,
        Commands::UploadDir(args) => handle_upload_dir(flags, &quark_pan, args).await?,
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

fn app_paths(config_file: Option<PathBuf>) -> Result<AppPaths, QuarkPanError> {
    if let Some(config_file) = config_file {
        let config_dir = config_file
            .parent()
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid --config-file path"))?
            .to_path_buf();
        return Ok(AppPaths {
            config_dir: config_dir.clone(),
            config_file,
            cookie_file: config_dir.join("cookie.txt"),
        });
    }
    let dirs = ProjectDirs::from("", "", "quarkpan").ok_or_else(|| {
        QuarkPanError::invalid_argument("cannot resolve platform config directory")
    })?;
    let config_dir = dirs.config_dir().to_path_buf();
    Ok(AppPaths {
        config_file: config_dir.join("config.toml"),
        cookie_file: config_dir.join("cookie.txt"),
        config_dir,
    })
}

async fn ensure_config_dir(paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
    tokio::fs::create_dir_all(&paths.config_dir).await?;
    Ok(())
}

async fn load_config(paths: &AppPaths) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !paths.config_file.exists() {
        return Ok(AppConfig::default());
    }
    let text = tokio::fs::read_to_string(&paths.config_file).await?;
    Ok(toml::from_str(&text)?)
}

async fn load_cookie(cli: &Cli, paths: &AppPaths) -> Result<String, QuarkPanError> {
    if let Some(cookie) = &cli.cookie {
        return Ok(cookie.clone());
    }
    if let Some(cookie_file) = &cli.cookie_file {
        let cookie = tokio::fs::read_to_string(cookie_file).await?;
        return Ok(cookie.trim().to_string());
    }
    if let Ok(cookie) = std::env::var("QUARK_COOKIE") {
        if !cookie.trim().is_empty() {
            return Ok(cookie);
        }
    }
    if paths.cookie_file.exists() {
        let cookie = tokio::fs::read_to_string(&paths.cookie_file).await?;
        return Ok(cookie.trim().to_string());
    }
    Err(QuarkPanError::missing_field("cookie"))
}

async fn handle_auth(
    flags: OutputFlags,
    paths: &AppPaths,
    args: AuthArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        AuthCommand::SetCookie(args) => {
            ensure_config_dir(paths).await?;
            let cookie = if let Some(cookie) = args.cookie {
                cookie
            } else if args.from_stdin {
                read_cookie_from_stdin()?
            } else if args.from_nano {
                edit_cookie_with("nano")?
            } else if args.from_vi {
                edit_cookie_with("vi")?
            } else {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "one of --cookie, --from-stdin, --from-nano, or --from-vi is required",
                )));
            };
            tokio::fs::write(&paths.cookie_file, format!("{}\n", cookie.trim())).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "persisted_cookie".to_string(),
                    path: Some(paths.cookie_file.display().to_string()),
                },
            )?;
        }
        AuthCommand::ImportCookie(args) => {
            ensure_config_dir(paths).await?;
            let cookie = tokio::fs::read_to_string(args.from_file).await?;
            tokio::fs::write(&paths.cookie_file, format!("{}\n", cookie.trim())).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "persisted_cookie".to_string(),
                    path: Some(paths.cookie_file.display().to_string()),
                },
            )?;
        }
        AuthCommand::ClearCookie => {
            remove_if_exists(&paths.cookie_file).await?;
            print_output(
                flags,
                &AuthSourceOutput {
                    source: "cleared".to_string(),
                    path: Some(paths.cookie_file.display().to_string()),
                },
            )?;
        }
        AuthCommand::ShowSource => {
            let output = if std::env::var("QUARK_COOKIE")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .is_some()
            {
                AuthSourceOutput {
                    source: "env".to_string(),
                    path: None,
                }
            } else if paths.cookie_file.exists() {
                AuthSourceOutput {
                    source: "persisted_cookie".to_string(),
                    path: Some(paths.cookie_file.display().to_string()),
                }
            } else {
                AuthSourceOutput {
                    source: "none".to_string(),
                    path: None,
                }
            };
            print_output(flags, &output)?;
        }
    }
    Ok(())
}

async fn handle_list(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: ListArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.all {
        let page = list_all_entries(quark_pan, &args.pdir_fid, args.size).await?;
        let page = ListPage {
            entries: page,
            page: 1,
            size: args.size,
            total: 0,
        };
        return print_list_output(flags, &page, args.long, args.raw_time);
    }
    if args.more {
        return handle_list_more(flags, quark_pan, args).await;
    }
    let page = quark_pan
        .list()
        .pdir_fid(args.pdir_fid)
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
            .pdir_fid(args.pdir_fid.clone())
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
    download_file(flags, quark_pan, &args).await
}

async fn download_file(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: &DownloadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.output.is_some() == args.stdout {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "exactly one of --output or --stdout is required",
        )));
    }
    let request = quark_pan.download().fid(args.fid.clone()).prepare()?;
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
    let output = args.output.clone().expect("checked above");
    let task_path = file_task_path(&output);
    if has_same_download_target(&output, info.md5.as_deref()).await? {
        cleanup_download_artifacts(&output, &task_path).await?;
        if !flags.quiet {
            eprintln!("download skipped: local file already matches remote md5");
        }
        return Ok(());
    }

    if let Some(task) = read_json_file::<DownloadTask>(&task_path).await? {
        let same_target = task.fid == args.fid && task.output_path == output.to_string_lossy();
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
    if args.overwrite && output.exists() && !args.continue_download {
        cleanup_download_artifacts(&output, &task_path).await?;
    }

    let task = DownloadTask {
        kind: "download".to_string(),
        fid: args.fid.clone(),
        output_path: output.to_string_lossy().to_string(),
        md5: info.md5.clone(),
    };
    write_json_file(&task_path, &task).await?;

    download_with_retry(
        flags,
        quark_pan,
        &args.fid,
        &output,
        args.continue_download,
        args.retry,
        args.retry_delay,
    )
    .await?;

    if let Some(md5) = info.md5.as_deref() {
        let local = md5_file(&output).await?;
        if !md5_matches_remote(&local, md5) {
            return Err(Box::new(QuarkPanError::invalid_argument(format!(
                "download completed but md5 mismatch: local={}, remote={}",
                local, md5
            ))));
        }
    }
    remove_if_exists(&task_path).await?;
    Ok(())
}

async fn download_with_retry(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    fid: &str,
    output: &Path,
    allow_continue: bool,
    retry: u32,
    retry_delay: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let control = if flags.no_progress || flags.quiet || !flags.interactive {
        None
    } else {
        let control = TransferControl::new(None);
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), progress_label("download", output));
        Some(control)
    };
    let mut attempts = 0_u32;
    loop {
        let start_offset = if allow_continue && output.exists() {
            tokio::fs::metadata(output).await?.len()
        } else if attempts > 0 && output.exists() {
            tokio::fs::metadata(output).await?.len()
        } else {
            0
        };

        let mut builder = quark_pan.download().fid(fid.to_string());
        if start_offset > 0 {
            builder = builder.start_offset(start_offset);
        }
        let raw_stream = builder.prepare()?.stream().await;
        let raw_stream = match raw_stream {
            Ok(stream) => stream,
            Err(err @ QuarkPanError::Cancelled) => return Err(Box::new(err)),
            Err(err) if attempts < retry => {
                attempts += 1;
                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                if !flags.quiet {
                    eprintln!("download stream retry {attempts}/{retry}: {err}");
                }
                continue;
            }
            Err(err) => return Err(Box::new(err)),
        };

        let mut file = if start_offset > 0 {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(output)
                .await?
        } else {
            tokio::fs::File::create(output).await?
        };

        let result = if let Some(control) = &control {
            let mut stream = ProgressStream::new(raw_stream, control.clone());
            write_stream_to_file(&mut stream, &mut file).await
        } else {
            let mut stream = raw_stream;
            write_stream_to_file(&mut stream, &mut file).await
        };
        file.flush().await?;

        match result {
            Ok(()) => {
                if control.is_some() {
                    if let Some(control) = &control {
                        control.finish();
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    eprintln!();
                }
                return Ok(());
            }
            Err(err)
                if err
                    .downcast_ref::<QuarkPanError>()
                    .is_some_and(|e| matches!(e, QuarkPanError::Cancelled)) =>
            {
                return Err(err);
            }
            Err(err) if attempts < retry => {
                attempts += 1;
                if !flags.quiet {
                    eprintln!("download retry {attempts}/{retry}: {err}");
                }
                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn handle_download_dir(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: DownloadDirArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let task_path = dir_task_path(&args.output)?;
    let existing_task = read_json_file::<DownloadDirTask>(&task_path).await?;
    let merge_mode = args.continue_download && args.overwrite;

    if args.output.exists() && !args.continue_download && !args.overwrite {
        return Err(Box::new(QuarkPanError::invalid_argument(format!(
            "output directory already exists: {}",
            args.output.display()
        ))));
    }
    if args.continue_download && existing_task.is_none() && args.output.exists() && !merge_mode {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "no interrupted directory task found; local directory already exists or download already completed",
        )));
    }

    tokio::fs::create_dir_all(&args.output).await?;
    let files = collect_remote_files(quark_pan, &args.pdir_fid, Path::new("")).await?;
    let mut task = existing_task.unwrap_or(DownloadDirTask {
        kind: "download_dir".to_string(),
        pdir_fid: args.pdir_fid.clone(),
        output_dir: args.output.to_string_lossy().to_string(),
        entries: files
            .iter()
            .map(|item| DownloadDirEntryTask {
                relative_path: item.relative_path.to_string_lossy().to_string(),
                fid: item.fid.clone(),
                md5: None,
                status: DirEntryStatus::Pending,
            })
            .collect(),
    });
    write_json_file(&task_path, &task).await?;

    for idx in 0..task.entries.len() {
        if matches!(
            task.entries[idx].status,
            DirEntryStatus::Done | DirEntryStatus::Skipped
        ) {
            continue;
        }
        let output_path = args.output.join(&task.entries[idx].relative_path);
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let info = quark_pan
            .download()
            .fid(task.entries[idx].fid.clone())
            .prepare()?
            .info()
            .await?;
        task.entries[idx].md5 = info.md5.clone();

        if merge_mode && has_same_download_target(&output_path, info.md5.as_deref()).await? {
            task.entries[idx].status = DirEntryStatus::Skipped;
            write_json_file(&task_path, &task).await?;
            continue;
        }
        if output_path.exists() && !merge_mode && !args.continue_download {
            return Err(Box::new(QuarkPanError::invalid_argument(format!(
                "local file already exists: {}",
                output_path.display()
            ))));
        }

        task.entries[idx].status = DirEntryStatus::Running;
        write_json_file(&task_path, &task).await?;
        let file_args = DownloadArgs {
            fid: task.entries[idx].fid.clone(),
            output: Some(output_path),
            stdout: false,
            overwrite: merge_mode,
            continue_download: args.continue_download,
            retry: args.retry,
            retry_delay: args.retry_delay,
        };
        match download_file(flags, quark_pan, &file_args).await {
            Ok(()) => task.entries[idx].status = DirEntryStatus::Done,
            Err(err) => {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(err);
            }
        }
        write_json_file(&task_path, &task).await?;
    }

    remove_if_exists(&task_path).await?;
    Ok(())
}

async fn handle_folder(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: FolderArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.command {
        FolderCommand::Create(args) => {
            let fid = quark_pan
                .create_folder()
                .pdir_fid(args.pdir_fid)
                .file_name(args.file_name)
                .prepare()?
                .request()
                .await?;
            print_output(flags, &FolderCreateOutput { fid })?;
        }
    }
    Ok(())
}

async fn handle_rename(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: RenameArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    quark_pan
        .rename()
        .fid(args.fid.clone())
        .file_name(args.file_name.clone())
        .prepare()?
        .request()
        .await?;
    print_output(
        flags,
        &RenameOutput {
            fid: args.fid,
            file_name: args.file_name,
        },
    )?;
    Ok(())
}

async fn handle_upload(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let task_path = file_task_path(&args.file);
    if args.r#continue {
        return resume_upload(flags, quark_pan, args, task_path).await;
    }

    let local = hash_file(&args.file, args.file_name.as_deref()).await?;
    let prepared = quark_pan
        .upload()
        .pdir_fid(args.pdir_fid.clone())
        .file_name(local.name.clone())
        .size(local.size)
        .md5(local.md5.clone())
        .sha1(local.sha1.clone())
        .prepare()
        .await?;

    match prepared {
        UploadPrepareResult::RapidUploaded { fid } => {
            remove_if_exists(&task_path).await?;
            print_output(
                flags,
                &UploadDoneOutput {
                    fid,
                    rapid_upload: true,
                },
            )?;
        }
        UploadPrepareResult::NeedUpload(session) => {
            let upload_task = UploadTask {
                kind: "upload".to_string(),
                file_path: args.file.to_string_lossy().to_string(),
                file_name: local.name.clone(),
                pdir_fid: args.pdir_fid,
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
                    fid: completed.fid,
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
    let Some(task) = read_json_file::<UploadTask>(&task_path).await? else {
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
            fid: completed.fid,
            rapid_upload: completed.rapid_upload,
        },
    )?;
    Ok(())
}

async fn handle_upload_dir(
    flags: OutputFlags,
    quark_pan: &QuarkPan,
    args: UploadDirArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_dir = tokio::fs::canonicalize(&args.dir).await?;
    let task_path = dir_task_path(&source_dir)?;
    let existing_task = read_json_file::<UploadDirTask>(&task_path).await?;
    let root_name = args.file_name.clone().unwrap_or_else(|| {
        source_dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("root")
            .to_string()
    });
    let merge_mode = args.r#continue && args.overwrite;

    let root_fid = if let Some(task) = &existing_task {
        task.root_fid.clone()
    } else {
        let existing = find_entry_by_name(quark_pan, &args.pdir_fid, &root_name).await?;
        match existing {
            Some(entry) if entry.dir && !args.r#continue && !args.overwrite => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "target cloud folder already exists",
                )));
            }
            Some(entry) if entry.dir && args.r#continue && !merge_mode => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "no interrupted directory task found; cloud folder already exists or upload already completed",
                )));
            }
            Some(entry) if entry.dir => entry.fid,
            Some(_) => {
                return Err(Box::new(QuarkPanError::invalid_argument(
                    "target cloud entry exists and is not a folder",
                )));
            }
            None => {
                quark_pan
                    .create_folder()
                    .pdir_fid(args.pdir_fid.clone())
                    .file_name(root_name.clone())
                    .prepare()?
                    .request()
                    .await?
            }
        }
    };

    let files = collect_local_files(&source_dir).await?;
    let mut task = existing_task.unwrap_or(UploadDirTask {
        kind: "upload_dir".to_string(),
        source_dir: source_dir.to_string_lossy().to_string(),
        pdir_fid: args.pdir_fid.clone(),
        target_file_name: root_name.clone(),
        root_fid: root_fid.clone(),
        entries: files
            .iter()
            .map(|item| UploadDirEntryTask {
                relative_path: item.relative_path.to_string_lossy().to_string(),
                status: DirEntryStatus::Pending,
            })
            .collect(),
    });
    write_json_file(&task_path, &task).await?;

    let mut folder_cache = HashMap::new();
    folder_cache.insert(PathBuf::new(), root_fid);

    for idx in 0..task.entries.len() {
        if matches!(
            task.entries[idx].status,
            DirEntryStatus::Done | DirEntryStatus::Skipped
        ) {
            continue;
        }
        task.entries[idx].status = DirEntryStatus::Running;
        write_json_file(&task_path, &task).await?;

        let relative_path = PathBuf::from(&task.entries[idx].relative_path);
        let absolute_path = source_dir.join(&relative_path);
        let parent_relative = relative_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        let remote_parent = ensure_remote_folder_chain(
            quark_pan,
            &mut folder_cache,
            &task.root_fid,
            &parent_relative,
        )
        .await?;
        let file_name = absolute_path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| QuarkPanError::invalid_argument("invalid file name"))?
            .to_string();

        if let Some(existing) = find_entry_by_name(quark_pan, &remote_parent, &file_name).await? {
            if !merge_mode {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(Box::new(QuarkPanError::invalid_argument(format!(
                    "cloud file already exists: {}",
                    relative_path.display()
                ))));
            }
            if existing.dir {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(Box::new(QuarkPanError::invalid_argument(format!(
                    "cloud entry is a folder but local path is a file: {}",
                    relative_path.display()
                ))));
            }
            let local = hash_file(&absolute_path, Some(&file_name)).await?;
            let remote = quark_pan
                .download()
                .fid(existing.fid.clone())
                .prepare()?
                .info()
                .await?;
            if let Some(md5) = remote.md5 {
                if md5.eq_ignore_ascii_case(&local.md5) {
                    task.entries[idx].status = DirEntryStatus::Skipped;
                    write_json_file(&task_path, &task).await?;
                    continue;
                }
            }
            quark_pan.delete(&existing.fid).await?;
        }

        let upload_args = UploadArgs {
            pdir_fid: remote_parent,
            file: absolute_path,
            file_name: Some(file_name),
            r#continue: false,
            overwrite: false,
        };
        match handle_upload(flags, quark_pan, upload_args).await {
            Ok(()) => task.entries[idx].status = DirEntryStatus::Done,
            Err(err) => {
                task.entries[idx].status = DirEntryStatus::Failed;
                write_json_file(&task_path, &task).await?;
                return Err(err);
            }
        }
        write_json_file(&task_path, &task).await?;
    }

    remove_if_exists(&task_path).await?;
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

    if flags.no_progress || flags.quiet || !flags.interactive {
        Ok(session
            .upload_stream_resumable(stream, state, on_part_uploaded)
            .await?)
    } else {
        let control = TransferControl::new(Some(total_remaining));
        spawn_ctrl_c_cancel(control.clone());
        spawn_progress_printer(control.clone(), progress_label("upload", file_path));
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

async fn list_all_entries(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    size: u32,
) -> Result<Vec<QuarkEntry>, Box<dyn std::error::Error>> {
    let mut page_no = 1;
    let mut entries = Vec::new();
    loop {
        let page = quark_pan
            .list()
            .pdir_fid(pdir_fid.to_string())
            .page(page_no)
            .size(size)
            .prepare()?
            .request()
            .await?;
        let count = page.entries.len();
        entries.extend(page.entries);
        if count < size as usize {
            break;
        }
        page_no += 1;
    }
    Ok(entries)
}

async fn collect_remote_files(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    prefix: &Path,
) -> Result<Vec<RemoteFileItem>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    let mut stack = vec![(pdir_fid.to_string(), prefix.to_path_buf())];
    while let Some((current_pdir_fid, current_prefix)) = stack.pop() {
        let entries = list_all_entries(quark_pan, &current_pdir_fid, 100).await?;
        for entry in entries {
            let path = current_prefix.join(&entry.file_name);
            if entry.dir {
                stack.push((entry.fid, path));
            } else {
                out.push(RemoteFileItem {
                    relative_path: path,
                    fid: entry.fid,
                });
            }
        }
    }
    Ok(out)
}

async fn collect_local_files(dir: &Path) -> Result<Vec<LocalFileItem>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    collect_local_files_inner(dir, dir, &mut out).await?;
    out.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(out)
}

async fn collect_local_files_inner(
    root: &Path,
    current: &Path,
    out: &mut Vec<LocalFileItem>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut dirs = vec![current.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            let meta = entry.metadata().await?;
            if meta.is_dir() {
                dirs.push(path);
            } else if meta.is_file() {
                out.push(LocalFileItem {
                    relative_path: path.strip_prefix(root)?.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

async fn find_entry_by_name(
    quark_pan: &QuarkPan,
    pdir_fid: &str,
    name: &str,
) -> Result<Option<QuarkEntry>, Box<dyn std::error::Error>> {
    let entries = list_all_entries(quark_pan, pdir_fid, 100).await?;
    Ok(entries.into_iter().find(|entry| entry.file_name == name))
}

async fn ensure_remote_folder_chain(
    quark_pan: &QuarkPan,
    cache: &mut HashMap<PathBuf, String>,
    root_fid: &str,
    relative: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if relative.as_os_str().is_empty() {
        return Ok(root_fid.to_string());
    }
    if let Some(found) = cache.get(relative) {
        return Ok(found.clone());
    }
    let mut current_rel = PathBuf::new();
    let mut current_id = root_fid.to_string();
    for component in relative.components() {
        current_rel.push(component.as_os_str());
        if let Some(found) = cache.get(&current_rel) {
            current_id = found.clone();
            continue;
        }
        let name = component.as_os_str().to_string_lossy().to_string();
        let next_id =
            if let Some(existing) = find_entry_by_name(quark_pan, &current_id, &name).await? {
                if !existing.dir {
                    return Err(Box::new(QuarkPanError::invalid_argument(format!(
                        "cloud path component exists as file: {}",
                        current_rel.display()
                    ))));
                }
                existing.fid
            } else {
                quark_pan
                    .create_folder()
                    .pdir_fid(current_id.clone())
                    .file_name(name)
                    .prepare()?
                    .request()
                    .await?
            };
        cache.insert(current_rel.clone(), next_id.clone());
        current_id = next_id;
    }
    Ok(current_id)
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

fn file_task_path(path: &Path) -> PathBuf {
    let base = path.as_os_str().to_string_lossy().to_string();
    PathBuf::from(format!("{base}.quark.task"))
}

fn dir_task_path(path: &Path) -> Result<PathBuf, QuarkPanError> {
    let parent = path
        .parent()
        .ok_or_else(|| QuarkPanError::invalid_argument("directory has no parent"))?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| QuarkPanError::invalid_argument("invalid directory name"))?;
    Ok(parent.join(format!("{name}.quark.task")))
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
    Ok(md5_matches_remote(&local_md5, remote_md5))
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
    let value = serde_json::to_value(data)?;
    if let Ok(upload) = serde_json::from_value::<UploadDoneOutput>(value.clone()) {
        let rendered = if upload.rapid_upload {
            format!("rapid upload completed: {}", upload.fid)
        } else {
            format!("upload completed: {}", upload.fid)
        };
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(rename) = serde_json::from_value::<RenameOutput>(value.clone()) {
        let rendered = format!("renamed {} -> {}", rename.fid, rename.file_name);
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(folder) = serde_json::from_value::<FolderCreateOutput>(value.clone()) {
        let rendered = format!("folder created: {}", folder.fid);
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else if let Ok(auth) = serde_json::from_value::<AuthSourceOutput>(value.clone()) {
        let rendered = match auth.path {
            Some(path) => format!("{}: {}", auth.source, path),
            None => auth.source,
        };
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    } else {
        let rendered = serde_json::to_string_pretty(&value)?;
        if flags.color {
            println!("{}", rendered.green());
        } else {
            println!("{rendered}");
        }
    }
    Ok(())
}

fn read_cookie_from_stdin() -> Result<String, Box<dyn std::error::Error>> {
    if std::io::stdin().is_terminal() {
        eprintln!("paste cookie, then press Enter:");
    }
    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin.read_line(&mut line)?;
    let cookie = line.trim().to_string();
    if cookie.is_empty() {
        return Err(Box::new(QuarkPanError::invalid_argument(
            "cookie cannot be empty",
        )));
    }
    Ok(cookie)
}

fn edit_cookie_with(editor: &str) -> Result<String, Box<dyn std::error::Error>> {
    let temp_path = temporary_cookie_path(editor);
    std::fs::write(&temp_path, b"")?;
    let status = Command::new(editor).arg(&temp_path).status()?;
    let result = if status.success() {
        let cookie = std::fs::read_to_string(&temp_path)?.trim().to_string();
        if cookie.is_empty() {
            Err(
                Box::new(QuarkPanError::invalid_argument("cookie cannot be empty"))
                    as Box<dyn std::error::Error>,
            )
        } else {
            Ok(cookie)
        }
    } else {
        Err(Box::new(QuarkPanError::invalid_argument(format!(
            "{editor} exited with status {status}"
        ))) as Box<dyn std::error::Error>)
    };
    let _ = std::fs::remove_file(&temp_path);
    result
}

fn temporary_cookie_path(editor: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("quarkpan-cookie-{editor}-{pid}-{nanos}.txt"))
}

fn md5_matches_remote(local_hex_md5: &str, remote_md5: &str) -> bool {
    if local_hex_md5.eq_ignore_ascii_case(remote_md5) {
        return true;
    }
    let Ok(raw) = decode_hex(local_hex_md5) else {
        return false;
    };
    general_purpose::STANDARD.encode(raw) == remote_md5
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, QuarkPanError> {
    if !hex.len().is_multiple_of(2) {
        return Err(QuarkPanError::invalid_argument("invalid md5 hex length"));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_value(bytes[i])?;
        let lo = hex_value(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Result<u8, QuarkPanError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(QuarkPanError::invalid_argument("invalid hex digit")),
    }
}

fn spawn_ctrl_c_cancel(control: TransferControl) {
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        control.cancel();
    });
}

fn spawn_progress_printer(control: TransferControl, label: String) {
    let progress_bar = create_progress_bar(&label, control.snapshot().total);
    tokio::spawn(async move {
        let mut rx = control.subscribe();
        while rx.changed().await.is_ok() {
            let progress = *rx.borrow();
            update_progress_bar(&progress_bar, &label, progress);
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
            "{spinner:.green} {msg:<28} [{bar:36.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, eta {eta})",
        )
        .unwrap()
        .progress_chars("=> "),
        None => ProgressStyle::with_template("{spinner:.green} {msg:<28} {bytes} ({bytes_per_sec})")
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

fn progress_label(action: &str, path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .map(|v| v.to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let text = format!("{action} {name}");
    truncate_label(&text, 28)
}

fn truncate_label(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            break;
        }
        out.push(ch);
        count += 1;
    }
    if text.chars().count() > max_chars && max_chars > 1 {
        out.pop();
        out.push('…');
    }
    out
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
