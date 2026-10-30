mod checkvist;
mod cli;
mod config;
mod error;
mod local;
mod lock;
mod mcp;
#[cfg(test)]
mod tests;
mod tools;
mod tui;
mod workspace;
mod workspace_tasks;
#[cfg(test)]
mod workspace_tests;

use checkvist::{CheckvistClient, CheckvistConfig};
use clap::Parser;
use config::Config;
use error::ToolError;
use local::LocalState;
use tools::Tools;
use workspace::Workspace;

fn main() -> std::process::ExitCode {
    // The CLI's own config, not the app's — see `config.rs`. Loaded before
    // anything else because both the API client and the local-state reader
    // resolve their settings out of it, behind the environment.
    let config = Config::load();
    let tools = Tools {
        client: CheckvistClient::new(CheckvistConfig::resolve(&config)),
        local: LocalState::resolve(&config),
        workspace: Workspace::resolve(&config),
    };

    // `--mcp-server` is accepted as a bare flag, not just as the `mcp`
    // subcommand, so a client config written for `Priority --mcp-server` (the app's name before Takt)
    // works unchanged when pointed at this binary instead.
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--mcp-server")
    {
        mcp::Server::new(tools).run();
        return std::process::ExitCode::SUCCESS;
    }

    let parsed = cli::Cli::parse();

    let outcome = match &parsed.command {
        // Bare `priority` opens the terminal UI. Every subcommand still
        // works exactly as before; `--help` still prints help.
        None => tui::run(tools),
        Some(cli::Command::Mcp) => {
            mcp::Server::new(tools).run();
            Ok(())
        }
        Some(cli::Command::Tools) => {
            cli::print_tools(parsed.json);
            Ok(())
        }
        Some(cli::Command::Auth { command }) => cli::run_auth(config, command),
        _ => cli::run(tools, &parsed),
    };

    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            report(&error);
            std::process::ExitCode::FAILURE
        }
    }
}

fn report(error: &ToolError) {
    eprintln!("takt: {error}");
    if let Some(status) = error.status {
        eprintln!("  HTTP {status}");
        // The one failure worth naming, because the fix is a command rather
        // than a puzzle: an expired or mistyped key looks identical to a
        // server problem otherwise.
        if status == 401 {
            eprintln!("  Check your credentials with:  takt auth status");
        }
    }
    if let Some(body) = &error.body {
        eprintln!("  {body}");
    }
}
