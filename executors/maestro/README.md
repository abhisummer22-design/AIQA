# Maestro executor fixture

`flow.yaml` is real Maestro (mobile.dev) flow syntax, kept here so the Rust
`MaestroExecutor` has something concrete to shell out to. It cannot be
verified end-to-end in this dev environment: there's no Android/iOS
emulator or device, and — worth flagging explicitly — this machine already
has an unrelated Python package that also installs a `maestro` command
(roboflow/maestro, an ML training CLI), which shadows the real mobile.dev
Maestro CLI on PATH.

That means a live run of this executor here will report a real `FAILED`
outcome (the wrong tool crashing on unexpected arguments), not a clean
"maestro not found" error. That's expected and is itself evidence the
adapter is doing genuine subprocess execution and exit-code/stderr capture,
not simulating a result — it just can't be validated against the real
mobile.dev Maestro CLI without installing it (and a device/emulator)
separately.
