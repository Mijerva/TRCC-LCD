# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Project setup, license and protocol notes.
- Core library: handshake, frame encoding, image fitting and rotation, USB transport.
- `trcc-lcd` command-line tool with `info`, `send` and `render`.
- CI with formatting, lints, tests and dependency checks.
- `trcc-lcd-tray` background app: tray menu, animated GIFs, settings file, start with Windows, reconnects after unplug or sleep.

### Fixed

- Pictures no longer disappear after 2 seconds. The tray app and `trcc-lcd send` resend the current frame every second. The `keep_alive_secs` setting is gone.
