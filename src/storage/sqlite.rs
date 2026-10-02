use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::whatsapp::models::{Message, MessageDirection};

const DATABASE_FILE: &str = "messages.sqlite";

/// Persistent local cache for WhatsApp messages.
///
/// Messages are keyed by `(session_id, message_id)` so multiple wpp sessions
/// can safely share the same SQLite database without colliding on WhatsApp
/// message identifiers.
pub struct LocalMessageStore {
  connection: Connection,
  path: PathBuf,
}

impl LocalMessageStore {
  /// Open the persistent message cache, creating its directory and schema.
  pub fn open() -> anyhow::Result<Self> {
    let path = Self::database_path()?;

    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(&path)?;
    initialize(&connection)?;

    Ok(Self { connection, path })
  }

  /// Return the path of the persistent cache database.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Insert or update a set of messages atomically.
  ///
  /// The operation is idempotent: syncing the same messages again updates
  /// their mutable fields instead of creating duplicates.
  pub fn upsert_messages(
    &mut self,
    session_id: &str,
    messages: &[Message],
  ) -> anyhow::Result<usize> {
    if messages.is_empty() {
      return Ok(0);
    }

    let transaction = self.connection.transaction()?;

    let mut statement = transaction.prepare(
      "INSERT INTO messages (
         session_id,
         message_id,
         chat_id,
         sender,
         recipient,
         body,
         kind,
         direction,
         author,
         timestamp,
         status
       ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
       ON CONFLICT(session_id, message_id) DO UPDATE SET
         chat_id = excluded.chat_id,
         sender = excluded.sender,
         recipient = excluded.recipient,
         body = excluded.body,
         kind = excluded.kind,
         direction = excluded.direction,
         author = excluded.author,
         timestamp = excluded.timestamp,
         status = excluded.status",
    )?;

    for message in messages {
      statement.execute(params![
        session_id,
        &message.id,
        &message.chat_id,
        &message.from,
        &message.to,
        &message.body,
        &message.kind,
        direction_as_str(message.direction),
        &message.author,
        message.timestamp,
        &message.status,
      ])?;
    }

    drop(statement);
    transaction.commit()?;

    Ok(messages.len())
  }

  fn database_path() -> anyhow::Result<PathBuf> {
    let dir = dirs::data_local_dir()
      .ok_or_else(|| {
        anyhow::anyhow!(
          "Cannot determine local data directory"
        )
      })?
      .join("wpp");

    Ok(dir.join(DATABASE_FILE))
  }

  /// Open an in-memory message store with the same schema as the
  /// persistent cache. Useful for tests and benchmarks.
  pub fn in_memory() -> anyhow::Result<Self> {
    let connection = Connection::open_in_memory()?;

    initialize(&connection)?;

    Ok(Self {
      connection,
      path: PathBuf::from(":memory:"),
    })
  }
}

fn initialize(connection: &Connection) -> anyhow::Result<()> {
  connection.execute_batch(
    "PRAGMA foreign_keys = ON;

     CREATE TABLE IF NOT EXISTS messages (
       session_id TEXT NOT NULL,
       message_id TEXT NOT NULL,
       chat_id TEXT NOT NULL,
       sender TEXT NOT NULL,
       recipient TEXT NOT NULL,
       body TEXT,
       kind TEXT NOT NULL,
       direction TEXT NOT NULL,
       author TEXT,
       timestamp INTEGER,
       status TEXT NOT NULL,
       PRIMARY KEY (session_id, message_id)
     );

     CREATE INDEX IF NOT EXISTS idx_messages_session_chat_timestamp
       ON messages (
         session_id,
         chat_id,
         timestamp,
         message_id
       );",
  )?;

  Ok(())
}

fn direction_as_str(
  direction: MessageDirection,
) -> &'static str {
  match direction {
    MessageDirection::Incoming => "incoming",
    MessageDirection::Outgoing => "outgoing",
  }
}

#[cfg(test)]
mod tests {
  use super::LocalMessageStore;
  use crate::whatsapp::models::{
    Message,
    MessageDirection,
  };

  fn message(id: &str, body: &str) -> Message {
    Message {
      id: id.to_string(),
      chat_id: "chat".to_string(),
      from: "from".to_string(),
      to: "to".to_string(),
      body: Some(body.to_string()),
      kind: "text".to_string(),
      direction: MessageDirection::Incoming,
      author: None,
      timestamp: Some(100),
      status: "received".to_string(),
    }
  }

  #[test]
  fn inserts_messages() {
    let mut store =
      LocalMessageStore::in_memory()
        .expect("open store");

    let messages = vec![
      message("1", "hello"),
      message("2", "world"),
    ];

    let count = store
      .upsert_messages(
        "session-1",
        &messages,
      )
      .expect("insert messages");

    assert_eq!(count, 2);

    let count: i64 = store
      .connection
      .query_row(
        "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
        ["session-1"],
        |row| row.get(0),
      )
      .expect("count rows");

    assert_eq!(count, 2);
  }

  #[test]
  fn same_message_is_updated_instead_of_duplicated() {
    let mut store =
      LocalMessageStore::in_memory()
        .expect("open store");

    store
      .upsert_messages(
        "session-1",
        &[message("1", "first")],
      )
      .expect("insert message");

    store
      .upsert_messages(
        "session-1",
        &[message("1", "updated")],
      )
      .expect("update message");

    let count: i64 = store
      .connection
      .query_row(
        "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
        ["session-1"],
        |row| row.get(0),
      )
      .expect("count rows");

    let body: String = store
      .connection
      .query_row(
        "SELECT body
         FROM messages
         WHERE session_id = ?1
           AND message_id = ?2",
        ["session-1", "1"],
        |row| row.get(0),
      )
      .expect("read body");

    assert_eq!(count, 1);
    assert_eq!(body, "updated");
  }

  #[test]
  fn sessions_do_not_collide_on_message_id() {
    let mut store =
      LocalMessageStore::in_memory()
        .expect("open store");

    store
      .upsert_messages(
        "session-1",
        &[message("same-id", "one")],
      )
      .expect("insert first session");

    store
      .upsert_messages(
        "session-2",
        &[message("same-id", "two")],
      )
      .expect("insert second session");

    let count: i64 = store
      .connection
      .query_row(
        "SELECT COUNT(*) FROM messages",
        [],
        |row| row.get(0),
      )
      .expect("count rows");

    assert_eq!(count, 2);
  }
}