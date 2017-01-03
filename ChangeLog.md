# Change Log
All notable changes to Garta project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/) 
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]
### Added
- Tile cache
- Persistence module
- Map loading from JSON files

## [0.0.4] - 2016-12-29
### Added
- Multi-threaded background tile loading
- Coordinates button completed
- logging (env_logger)

### Fixed
- Maps button fixes

## [0.0.3] - 2016-12-22
### Added
- Maps button completed
- Layers button completed

### Changed
- code clean-up and refactoring (memory management in core module)

## [0.0.2] - 2016-12-17
### Added
- included a sample GPX file for unit tests

### Changed
- cleaned the code to make it free from warnings (excluding `dead_code` and `unused_variables`)
- relicensed the source code under GPLv3

## [0.0.1] - 2016-12-15
### Added
- pretty complete Mercator projection and location related math
- the main window
- basic GPX loading (for unit tests only)
- initial domain model
- basic singleton-like settings data structure
- a lot of unfinished code and far too many `unwrap` calls

