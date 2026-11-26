use clap::Parser;
use takt_cli::checkvist::{CheckvistClient, CheckvistConfig};
use takt_cli::config::Config;
use takt_cli::error::ToolError;
use takt_cli::local::LocalState;
use takt_cli::tools::Tools;
use takt_cli::workspace::Workspace;
use takt_cli::{cli, mcp, tui};

fn main() -> std::process::ExitCode {
    // The CLI's own config, not the app's — see `config.rs`. Loaded before
    // anything else because both the API client and the local-state reader
    // resolve their settings out of it, behind the environment.
    let config = Config::load();
    // Device conditions hold on a device of their kind; the CLI and the
    // MCP server run on the Mac, so this is a laptop, as the app is.
    takt_core::conditions::set_this_device_kind(Some("laptop".into()));
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
