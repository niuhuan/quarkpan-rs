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
- Added resumable CLI transfer flows backed by `.quark.task` files.
- Added transfer progress reporting and Ctrl+C cancellation support.
- Added `quarkpan` CLI with `list`, `folder create`, `download`, and `upload`.
