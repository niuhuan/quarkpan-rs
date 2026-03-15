# Changelog

All notable changes to this project will be documented in this file.

The format loosely follows Keep a Changelog, and this workspace currently uses semantic versioning.

## [0.1.0] - 2026-03-15

### Added

- Added `libquarkpan` as the async core library for Quark Drive.
- Added `QuarkPan::builder()` as the main client entry point.
- Added folder creation and folder listing support.
- Added file download by file ID with stream-based consumption.
- Added upload preparation, rapid-upload detection, and chunked upload support.
- Added `quarkpan` CLI with `list`, `folder create`, `download`, and `upload`.

## [0.1.1] - 2026-03-15

### Added

- Added resumable CLI transfer flows backed by `.quark.task` files.
- Added transfer progress reporting and Ctrl+C cancellation support.
- Added persistent cookie support for the CLI using the platform config directory.
- Added `auth` CLI commands for saving, importing, clearing, and inspecting cookie source.
- Added `download-dir` and `upload-dir` commands with directory task files.
- Added explicit retry logic for interrupted downloads and multipart uploads.
- Added file deletion support in `libquarkpan` for overwrite workflows.
- Expanded `quarkpan` CLI with `auth`, `download-dir`, and `upload-dir`.

### Changed

- Removed `reqwest-retry` and `reqwest-middleware` in favor of explicit retry control.
- Changed upload prepare flow to a single `prepare().await` call.
- Standardized directory task files to use a sibling `目录名.quark.task` naming scheme.
- Ctrl+C now stops active transfer attempts immediately and preserves task files for resume.
- Removed the CLI `--json` mode and standardized on human-readable output.
- `auth set-cookie` now uses explicit input sources: `--cookie`, `--from-stdin`, `--from-nano`, and `--from-vi`.
- Progress bars are now shown only on interactive TTY sessions and include the current file name.
- Upload completion now flushes the progress bar to a final 100% state before finishing.
- Download md5 verification now supports Quark's base64-encoded remote md5 values.
- `auth set-cookie --from-stdin` now prints a prompt before reading a single-line cookie.
