# Changelog

## v1.0.0 - 2026-09-19

### Added

- Add `Lettermint::email` for the Sending API.
- Add `Lettermint::api` for the Team API.
- Add generated Sending and Team API types and endpoint clients.
- Add scheduled delivery, typed message tags, a custom transport interface, and webhook verification.
- Add a 0.3-to-1.0 migration guide.

### Changed

- Replace the 0.3 public client interface with the new SDK interface.
- Generate API source from the central Lettermint OpenAPI specifications without copying the specifications into this repository.

Applications that use `lettermint = "0.3"` continue to resolve to the compatible 0.3 release line.

## Unreleased

- Initial Rust SDK with async sending client, team API client, generated serde DTOs, webhook verification, tests, and release workflows.
