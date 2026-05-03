# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.3.0](https://github.com/conradludgate/clashmap/compare/clashmap-v1.2.1...clashmap-v1.3.0) - 2026-05-03

### Added

- split clashcore (1.0.0) out of clashmap (1.3.0)

### Other

- *(clashcore)* final 1.0 polish
- *(clashcore)* rename Ref::new/into_parts to from_raw_parts/into_raw_parts
- *(clashcore)* tighten the public API for the 1.0 release
- document the safety contract of the detached lock guards
- replace internal field access with helper methods
- convert to Cargo workspace with crates/clashmap
