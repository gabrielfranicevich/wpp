use clap::{Parser, Subcommand, ValueEnum};

pub mod chat;
pub mod list;
pub mod listen;
pub mod login;
pub mod logout;
pub mod search;
pub mod send;
pub mod session;
pub mod switch;
pub mod sync;

#[derive(Parser)]
#[command(name = "wpp", about = "WhatsApp from the terminal", version)]
pub struct Cli {
  #[command(subcommand)]
  pub command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ChatFilter {
  Unread,
}

#[derive(Subcommand)]
pub enum Command {
  /// Log in to WhatsApp (QR code or phone pairing)
  Login {
    /// Phone number for pairing code (or omit / pass 'qr' for QR code)
    #[arg(value_name = "PHONE")]
    phone_pos: Option<String>,

    /// Use pairing code with this phone number instead of QR
    #[arg(short, long)]
    phone: Option<String>,

    /// Request QR code login explicitly
    #[arg(short, long)]
    qr: bool,

    /// Alias for this session; if omitted, one is generated after authentication
    #[arg(short, long)]
    alias: Option<String>,
  },

  /// Log out a WhatsApp session
  Logout {
    /// Session alias or phone number; omit to use the active session
    target: Option<String>,
  },

  /// Switch active session or list all sessions
  Switch {
    /// Session alias or phone number (omit to list)
    target: Option<String>,
  },

  /// List or delete saved WhatsApp sessions
  Session {
    /// Delete the selected saved session
    #[arg(short = 'D', long = "delete", value_name = "TARGET")]
    delete: Option<String>,
  },

  /// List chats (most recent first)
  List {
    /// Maximum number of chats to inspect
    #[arg(long, conflicts_with = "all")]
    limit: Option<usize>,

    /// Fetch all chats
    #[arg(long)]
    all: bool,

    /// Return up to N unread chats
    #[arg(long, conflicts_with = "filter")]
    unread: bool,

    /// Filter the selected chats
    #[arg(long, value_enum)]
    filter: Option<ChatFilter>,
  },

  /// Open or manage a chat
  Chat {
    /// Contact name or phone number
    who: String,

    /// Show only unread messages (oldest first)
    #[arg(long)]
    unread: bool,

    /// Delete the chat
    #[arg(short = 'd', long = "delete")]
    delete: bool,

    /// Block the contact
    #[arg(long)]
    block: bool,

    /// Unblock the contact
    #[arg(long)]
    unblock: bool,

    /// Archive the chat
    #[arg(long)]
    archive: bool,

    /// Unarchive the chat
    #[arg(long)]
    unarchive: bool,

    /// Pin the chat
    #[arg(long)]
    pin: bool,

    /// Unpin the chat
    #[arg(long)]
    unpin: bool,

    /// Mute the chat indefinitely
    #[arg(long)]
    mute: bool,

    /// Unmute the chat
    #[arg(long)]
    unmute: bool,

    /// Mark the chat as read
    #[arg(long)]
    mark_read: bool,

    /// Mark the chat as unread
    #[arg(long)]
    mark_unread: bool,
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

  /// Synchronize messages into the local SQLite cache
  Sync {
    /// Max messages per chat
    #[arg(long)]
    limit: Option<usize>,

    /// Only sync these contacts (comma-separated names or numbers)
    #[arg(long)]
    who: Option<String>,
  },
}
