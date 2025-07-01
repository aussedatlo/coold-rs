mod daemon;
mod api;
mod cli;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use daemon::{create_config, FanController};
use api::start_api;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coold-rs")]
#[command(about = "Fan control daemon with REST API and CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the daemon with REST API
    Daemon,
    /// Use CLI to interact with the daemon
    Cli {
        #[command(subcommand)]
        cli_command: cli::CliCommands,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Some(Commands::Cli { cli_command }) => {
            // Run CLI mode
            cli::run_cli(cli_command).await?;
        }
        Some(Commands::Daemon) | None => {
            // Run daemon mode (default)
            run_daemon().await?;
        }
    }
    
    Ok(())
}

async fn run_daemon() -> std::io::Result<()> {
    println!("Starting coold-rs fan control daemon with REST API...");

    let config = create_config();
    let controller = FanController::new(config);
    let running = controller.get_running();
    let running_clone = running.clone();

    // Set up Ctrl+C handler
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down gracefully...");
        running_clone.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl+C handler");

    // Start the fan control daemon in a separate thread
    let controller_clone = controller.clone();
    let daemon_handle = thread::spawn(move || {
        controller_clone.run();
    });

    // Start the REST API server
    let api_handle = start_api(controller, 8080);

    // Wait for either the daemon or API to finish
    tokio::select! {
        _ = api_handle => {
            println!("API server stopped");
        }
        _ = tokio::task::spawn_blocking(move || daemon_handle.join()) => {
            println!("Daemon stopped");
        }
    }

    println!("Shutdown complete.");
    Ok(())
}
