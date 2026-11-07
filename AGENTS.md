# Local installation

After every change, supersede the local Takt installation before finishing the task. Run `bash scripts/install_local.sh` from the repository root to build the Rust app (`desktop/`) in release, supersede `/Applications/Takt.app` with it (no backup is kept; the Swift app, an interim `/Applications/Takt Desktop.app` and an old `/Applications/Priority.app` are quit and removed), and relaunch it. `scripts/install_swift.sh` installs the Swift app there instead. Verify that installation succeeded and report any failure. Do not treat a successful build alone as completion.

The user has authorized this local installation workflow; do not ask for confirmation again. Request sandbox escalation when needed to write to `/Applications` and launch the app.
