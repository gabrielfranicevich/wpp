use clap::Parser;

mod app;
mod cli;
mod error;
mod terminal;
mod whatsapp;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
  let args = cli::Cli::parse();

  match args.command {
    cli::Command::Login {
      phone_pos,
      phone,
      qr,
      alias,
    } => {
      let phone = if qr {
        None
      } else {
        phone.or(phone_pos).filter(|p| {
          let p_lower = p.trim().to_lowercase();

          p_lower != "qr" && p_lower != "-qr" && p_lower != "--qr" && p_lower != "-q"
        })
      };

      cli::login::run(phone, alias).await?;
    }

    cli::Command::Logout { target } => {
      cli::logout::run(target).await?;
    }

    cli::Command::Switch { target } => {
      cli::switch::run(target)?;
    }

    cli::Command::Session { delete } => {
      cli::session::run(delete).await?;
    }

    cli::Command::List {
      limit,
      all,
      unread,
      filter,
    } => {
      cli::list::run(limit, all, unread, filter).await?;
    }

    cli::Command::Chat {
      who,
      unread,
      delete,
      block,
      unblock,
    } => {
      cli::chat::run(
        who,
        unread,
        delete,
        block,
        unblock,
      )
      .await?;
    }

    cli::Command::Search { query } => {
      cli::search::run(query).await?;
    }

    cli::Command::Send { who, message } => {
      cli::send::run(who, message).await?;
    }

    cli::Command::Listen
    | cli::Command::Sync { .. } => {
      eprintln!("Not yet implemented — coming in the next sprint.");
    }
  }

  Ok(())
}