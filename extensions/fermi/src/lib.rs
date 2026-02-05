use zed_extension_api::{self as zed, LanguageServerId, Result};

struct FermiExtension;

impl zed::Extension for FermiExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // Use absolute path to fermi-lsp binary
        let lsp_path = "/home/ilabra/fermi/fermi-lsp/target/release/fermi-lsp";

        Ok(zed::Command {
            command: lsp_path.to_string(),
            args: vec![],
            env: Default::default(),
        })
    }

    fn run_slash_command(
        &self,
        command: zed::SlashCommand,
        args: Vec<String>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput> {
        match command.name.as_str() {
            "run-forecast" => {
                let file_path = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "your-forecast.fpl".to_string());

                let root = worktree
                    .map(|wt| wt.root_path())
                    .unwrap_or_else(|| ".".to_string());

                let output = format!(
                    "# Run FPL Forecast\n\n\
                    To execute your forecast, use the terminal:\n\n\
                    ```bash\n\
                    # From your project root:\n\
                    cargo run --release {}\n\n\
                    # Or if fermi is built:\n\
                    ./target/release/fermi {}\n\
                    ```\n\n\
                    **Current workspace:** `{}`\n\n\
                    **Tip:** You can also use the integrated terminal in Zed (Cmd+J or Ctrl+J) to run forecasts.",
                    file_path, file_path, root
                );

                Ok(zed::SlashCommandOutput {
                    text: output,
                    sections: vec![],
                })
            }
            _ => Err(format!("Unknown command: {}", command.name))?,
        }
    }
}

zed::register_extension!(FermiExtension);
