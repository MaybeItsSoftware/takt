use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use takt_sync_server::auth::Verifier;
use takt_sync_server::config::Config;
use takt_sync_server::devices::{AccountAdmin, SupabaseAdmin};
use takt_sync_server::{AppState, MIGRATOR, notify, realtime, router};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // JSON lines, because Cloud Logging indexes the fields.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;
    let verifier = Arc::new(Verifier::for_project(
        &config.supabase_url,
        config.supabase_jwt_secret.as_deref(),
    ));
    let admin = match config.supabase_secret_key {
        Some(key) => {
            Some(Arc::new(SupabaseAdmin::new(&config.supabase_url, key)) as Arc<dyn AccountAdmin>)
        }
        None => {
            tracing::warn!("deleting accounts is off: set SUPABASE_SECRET_KEY");
            None
        }
    };

    // Supabase's pooler in session mode, or a direct connection: LISTEN needs
    // a session, so not the transaction pooler on port 6543.
    // Long-polls hold no connection while they wait, so a small pool serves
    // many devices.
    //
    // Everything lives in the `sync` schema, which Supabase's Data API
    // doesn't expose: only this server reads or writes it.
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(|connection, _| {
            Box::pin(async move {
                // Postgres resolves the path when it is used, not when it is
                // set, so this holds on the first connection too, before the
                // schema below exists.
                sqlx::query("SET search_path TO sync")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&config.database_url)
        .await?;
    // Once, before the migrations, rather than on every new connection.
    sqlx::query("CREATE SCHEMA IF NOT EXISTS sync")
        .execute(&pool)
        .await?;
    MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");

    let changes = notify::spawn_listener(&pool).await?;
    let realtime = realtime::prepare(&pool).await;
    tracing::info!(realtime, "realtime nudges");
    let (stop, shutdown) = watch::channel(false);
    let state = AppState {
        pool,
        verifier,
        admin,
        changes,
        shutdown,
        realtime,
    };

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(port = config.port, "listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutting down");
            let _ = stop.send(true);
        })
        .await?;
    Ok(())
}

/// SIGTERM is what Cloud Run sends on redeploy; ctrl-c is for running locally.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "could not listen for ctrl-c");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "could not listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
