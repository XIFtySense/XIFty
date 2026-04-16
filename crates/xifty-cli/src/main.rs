use clap::{Parser, Subcommand, ValueEnum};
use xifty_core::{ViewMode, XiftyError};
use xifty_json::{to_json_analysis, to_json_probe};

#[derive(Debug, Parser)]
#[command(name = "xifty")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Probe {
        path: std::path::PathBuf,
    },
    Extract {
        path: std::path::PathBuf,
        #[arg(long, value_enum)]
        view: Option<ViewArg>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ViewArg {
    Raw,
    Interpreted,
    Normalized,
    Report,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Probe { path } => xifty_cli::probe_path(path).and_then(|output| {
            to_json_probe(&output).map_err(|error| XiftyError::Parse {
                message: error.to_string(),
            })
        }),
        Command::Extract { path, view } => {
            let view_mode = match view {
                None => ViewMode::Full,
                Some(ViewArg::Raw) => ViewMode::Raw,
                Some(ViewArg::Interpreted) => ViewMode::Interpreted,
                Some(ViewArg::Normalized) => ViewMode::Normalized,
                Some(ViewArg::Report) => ViewMode::Report,
            };
            xifty_cli::extract_path(path, view_mode).and_then(|output| {
                to_json_analysis(&output).map_err(|error| XiftyError::Parse {
                    message: error.to_string(),
                })
            })
        }
    };

    match result {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
