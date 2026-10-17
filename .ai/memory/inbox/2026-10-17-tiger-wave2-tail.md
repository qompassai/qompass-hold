# Wave-1 tail bundle (Decision 7f) — strict-clippy burndown, 2026-10-17

Warnings: 140 -> 16 (both targets counted; 70 -> 8 unique).
- Mechanical classes cleared by clippy --fix + review: explicit
  auto-deref, needless borrow / question-mark / return / closure,
  borrow-deref-ref, redundant static lifetimes, into-iter-on-ref,
  collapsible-if, len-zero, comparison-to-empty.
- result_large_err (54 -> 16): the crate Error now boxes its large
  variants (Dbus(Box<zbus::Error>), Redb(Box<redb::Error>)) with
  From impls boxing at conversion. enum_variant_names cleared by
  renaming the variants (Io/Dbus/Redb/Gpg) — wire names are string
  literals in dbus_error_name and are unchanged (test-pinned).
- suspicious_open_options cleared with real semantics: the gpg entry
  writer now truncates (a shorter ciphertext previously left stale
  trailing bytes); the read/write store handle explicitly does not.
- RESIDUAL (8 unique, all clippy::result_large_err): secret_store.rs
  internal iterator/spawn_blocking closures returning
  Result<_, redb::Error> (160 B). Boxing there is not a type-alias
  change: redb's sub-error types (Transaction/Table/Storage/Commit)
  convert only into redb::Error, so each closure needs its call
  sites converted through into_result()/map_err individually. That
  is the remaining burndown pass; the crate's own Error is small.
Gates: cargo test 21/21, cargo fmt --check clean.
