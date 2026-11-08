# Local installation

After every change, supersede the local Takt installation before finishing the task. Run `bash scripts/install_local.sh` from the repository root to build the Rust app (`desktop/`) in release, supersede `/Applications/Takt.app` with it (no backup is kept; whatever is there, an interim `/Applications/Takt Desktop.app` and an old `/Applications/Priority.app` are quit and removed), and relaunch it. The Swift Mac app was retired on 2026-11-08; git history is the way back to it. Verify that installation succeeded and report any failure. Do not treat a successful build alone as completion.

The user has authorized this local installation workflow; do not ask for confirmation again. Request sandbox escalation when needed to write to `/Applications` and launch the app.
