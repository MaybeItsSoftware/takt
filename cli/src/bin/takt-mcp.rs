//! `takt-mcp`: the hosted MCP server (`src/remote/`, docs/remote-mcp.md).
//! Built with `--features remote`; Cloud Run runs it as `takt-mcp`.

use takt_cli::remote;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let config = match remote::Config::from_env() {
        Ok(config) => config,
        Err(problem) => {
            remote::log("error", &problem);
            return std::process::ExitCode::FAILURE;
        }
    };
    // A device condition holds only on a device of its kind; this server is
    // no device in anyone's hand, so none holds here (the core's default,
    // said out loud).
    takt_core::conditions::set_this_device_kind(None);
    let port = config.port;
    let app = remote::router(remote::state(config));
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            remote::log("error", &format!("could not listen on {port}: {error}"));
            return std::process::ExitCode::FAILURE;
        }
    };
    remote::log("info", &format!("listening on {port}"));
    let served = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await;
    match served {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            remote::log("error", &format!("the server stopped: {error}"));
            std::process::ExitCode::FAILURE
        }
    }
}

/// Ctrl-C, or the SIGTERM Cloud Run sends before it stops an instance.
async fn shutdown() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
