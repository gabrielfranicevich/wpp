use clap::Parser;

mod app;
mod cli;
mod error;
mod terminal;
mod whatsapp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

    match args.command {
        cli::Command::Login { phone, alias } => {
            cli::login::run(phone, alias).await?;
        }
        cli::Command::Switch { target } => {
            cli::switch::run(target)?;
        }
        cli::Command::Chats { .. }
        | cli::Command::Open { .. }
        | cli::Command::Send { .. }
        | cli::Command::Search { .. }
        | cli::Command::Listen
        | cli::Command::Sync { .. } => {
            eprintln!("Not yet implemented — coming in the next sprint.");
        }
    }

    Ok(())
}
