# Changelog

## 0.1.0

First release as a crate (the code was vendored by apps before).

- The worker reads the variables the platform injects into the guest pod as `PICUSBASE_MEDIATOR_TCP_ADDR`,
  `PICUSBASE_PROJECT_REF` and `PICUSBASE_FUNCTION_ID`. The old `PICUS_<name>` names are still read when the new
  one is unset, so a function built with this version keeps working on a cluster older than the rename
  (2026-11-16). A build that only reads the old names fails every database or HTTP call with
  "mediator address not set" on a cluster that injects the new ones.
