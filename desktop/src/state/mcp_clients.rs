//! Adding Takt's MCP server to the MCP clients on this Mac: Claude Desktop,
//! Claude Code, Cursor, VS Code and the rest the core knows
//! (`client_config::mcp_client_catalog`). `MCPClientInstaller.swift` and
//! `IntegrationCoordinator.setUpMCPClient`.
//!
//! The entry names the `takt` bundled in this app with `mcp`, and carries no
//! secret: the server reads Checkvist's credentials from the CLI's own file,
//! which is seeded from the keychain first when there are any. A client
//! that owns its config gets a command for Terminal on the clipboard, and
//! one whose config has comments gets a snippet to paste.

use super::App;
use std::collections::HashMap;
use std::path::PathBuf;
use takt_core::client_config::{
    self, ConfigWriteOutcome, McpClientDescriptor, McpClientInstallStyle, McpServerEntry,
};

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn config_path(client: &McpClientDescriptor) -> PathBuf {
    client
        .config_path_components
        .iter()
        .fold(home(), |path, part| path.join(part))
}

/// The clients worth offering: their config, a marker or the app is here.
pub fn detected() -> Vec<McpClientDescriptor> {
    client_config::mcp_client_catalog()
        .into_iter()
        .filter(|client| {
            config_path(client).exists()
                || client
                    .home_relative_markers
                    .iter()
                    .any(|marker| home().join(marker).exists())
                || client.application_bundle_names.iter().any(|bundle| {
                    PathBuf::from("/Applications").join(bundle).exists()
                        || home().join("Applications").join(bundle).exists()
                })
        })
        .collect()
}

/// Whether the client's config already names the server.
fn installed(client: &McpClientDescriptor) -> bool {
    std::fs::read_to_string(config_path(client))
        .ok()
        .and_then(|text| client_config::parse_config_object(Some(&text)))
        .and_then(|root| root.get(&client.servers_key).cloned())
        .and_then(|servers| servers.get(client_config::mcp_server_name()).cloned())
        .is_some()
}

fn entry(client: &McpClientDescriptor) -> Option<McpServerEntry> {
    Some(McpServerEntry {
        command: super::agent::helper_path()?,
        args: vec!["mcp".into()],
        env: HashMap::new(),
        transport_type: client.requires_transport_type.then(|| "stdio".into()),
    })
}

fn copy(text: &str) -> bool {
    use std::io::Write;
    let Ok(mut child) = std::process::Command::new("/usr/bin/pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return false;
    };
    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok() && wrote
}

impl App {
    pub fn mcp_client_rows(&self) -> Vec<(String, String, String)> {
        detected()
            .into_iter()
            .map(|client| {
                let state = if installed(&client) {
                    "Added".to_string()
                } else {
                    match client.install_style {
                        McpClientInstallStyle::MergeConfigFile => "Return to add".into(),
                        McpClientInstallStyle::TerminalCommand => "Copies a command".into(),
                        McpClientInstallStyle::PasteSnippet => "Copies a snippet".into(),
                    }
                };
                (
                    format!("mcp:{}", client.id),
                    format!("Add Takt to {}", client.display_name),
                    state,
                )
            })
            .collect()
    }

    /// Return on a client's row.
    pub fn install_mcp_client(&mut self, id: &str) {
        let Some(client) = client_config::mcp_client_catalog()
            .into_iter()
            .find(|c| c.id == id)
        else {
            return;
        };
        let Some(entry) = entry(&client) else {
            self.message = Some("There's no takt here to name: install the app first.".into());
            return;
        };
        if let Some(credentials) = super::checkvist::credentials() {
            let seeded = client_config::seed_cli_config(
                home().to_string_lossy().into_owned(),
                client_config::CliCredentials {
                    username: credentials.username,
                    remote_key: credentials.remote_key,
                    list_id: String::new(),
                },
            );
            if let Err(error) = seeded {
                self.message = Some(error.to_string());
                return;
            }
        }
        let note = &client.post_install_note;
        self.message = Some(match client.install_style {
            McpClientInstallStyle::MergeConfigFile => {
                let path = config_path(&client);
                let existing = std::fs::read_to_string(&path).ok();
                let merged = client_config::mcp_config_merged(
                    entry,
                    client_config::mcp_server_name(),
                    existing,
                    client.servers_key.clone(),
                    path.to_string_lossy().into_owned(),
                );
                match merged {
                    Err(error) => error.to_string(),
                    Ok(write) if write.outcome == ConfigWriteOutcome::Unchanged => {
                        format!("{} is already set up.", client.display_name)
                    }
                    Ok(write) => {
                        let saved = path
                            .parent()
                            .map(std::fs::create_dir_all)
                            .transpose()
                            .map_err(|e| e.to_string())
                            .and_then(|_| {
                                std::fs::write(&path, write.contents).map_err(|e| e.to_string())
                            });
                        match saved {
                            Ok(()) if write.outcome == ConfigWriteOutcome::Added => {
                                format!("Added to {}. {note}", client.display_name)
                            }
                            Ok(()) => {
                                format!("Updated the entry in {}. {note}", client.display_name)
                            }
                            Err(error) => format!("Couldn't write {}: {error}", path.display()),
                        }
                    }
                }
            }
            McpClientInstallStyle::TerminalCommand => {
                let command =
                    client_config::mcp_terminal_command(entry, client_config::mcp_server_name());
                if copy(&command) {
                    format!("Command copied. Paste it into Terminal. {note}")
                } else {
                    command
                }
            }
            McpClientInstallStyle::PasteSnippet => {
                let snippet = client_config::mcp_paste_snippet(
                    entry,
                    client.servers_key.clone(),
                    client_config::mcp_server_name(),
                );
                copy(&snippet);
                format!(
                    "Snippet copied. Paste it inside the outer braces of {}. {note}",
                    config_path(&client).display()
                )
            }
        });
    }
}
