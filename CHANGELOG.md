# Changelog
All notable changes to this project will be documented in this file.
 
The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).
 
## [Unreleased]
 
### Added
- AddressSanitizer CI workflow to catch FFI memory bugs in the Rust ↔ Python boundary (#10)
 
### Changed
 
### Fixed
- Gemspec was excluding `.rake` files from the built gem. So installed gems didn't include `rake rubyx:init`, `rake rubyx:check`, `rake rubyx:status`, `rake rubyx:packages`, or `rake rubyx:clear_cache`.

## [0.2.0] - 2026-03-26
Initial release covered by this changelog. See git history for changes prior to 0.2.0.

[Unreleased]: https://github.com/yinho999/rubyx/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/yinho999/rubyx/releases/tag/v0.2.0
