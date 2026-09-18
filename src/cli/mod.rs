use clap::{Parser, Subcommand};

pub mod login;
pub mod switch;

#[derive(Parser)]
#[command(name = "wpp", about = "WhatsApp from the terminal", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Log in to WhatsApp (QR code or phone pairing)
    Login {
        /// Use pairing code with this phone number instead of QR
        #[arg(long)]
        phone: Option<String>,
        /// Alias for this session (e.g. "personal", "business")
        #[arg(long, default_value = "default")]
        alias: String,
    },
    /// Switch active session or list all sessions
    Switch {
        /// Session alias or phone number (omit to list)
        target: Option<String>,
    },
    /// List chats (most recent first)
    Chats {
        /// Show only unread chats
        #[arg(long)]
        unread: bool,
    },
    /// Open a chat history
    Open {
        /// Contact name or phone number
        who: String,
        /// Show only unread messages (oldest first)
        #[arg(long)]
        unread: bool,
    },
    /// Send a text message
    Send {
        /// Contact name or phone number
        who: String,
        /// Message text (multiple words joined)
        message: Vec<String>,
    },
    /// Search chats by name or number
    Search {
        /// Search query
        query: String,
    },
    /// Listen for incoming messages in real time
    Listen,
    /// Sync messages to local cache
    Sync {
        /// Max messages per chat
        #[arg(long)]
        limit: Option<usize>,
        /// Only sync these contacts (comma-separated names or numbers)
        #[arg(long)]
        who: Option<String>,
    },
}
