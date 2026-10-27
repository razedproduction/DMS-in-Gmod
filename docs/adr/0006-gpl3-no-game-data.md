# 0006. GPL-3.0-only, GTA IV's data isn't distributed

Status: accepted (2026-10-21)

## Context

The Delirium SDK is distributed under GPL-3.0-only, and the module links it statically. Delirium needs the character
(`fred.xrt` / `character.json`), the build and the task tuning (`taskparams.txt`) from the user's copy of GTA IV.

## Decision

- The whole project (the module, the Lua addon, the tools) is GPL-3.0-only, the text in `LICENSE`.
- Neither the repository nor the releases have GTA IV's files or data extracted from them. The user puts their own in
  `garrysmod/data/dms/` (instructions: `docs/data-setup.md`, milestone M2).

## Consequences

- The module's source is published together with the binaries.
- Without the user's data the addon loads but creates no agents and says which files are missing.
