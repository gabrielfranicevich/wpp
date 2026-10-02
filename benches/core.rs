//! CPU-bound hot paths of wpp: chat search, message selection,
//! terminal text layout, OpenWA payload decoding and local SQLite caching.

use divan::{black_box, Bencher};

use wpp::app::services::chats::{matches_search_query, normalize_phone};
use wpp::app::services::messages::take_unread_messages;
use wpp::storage::LocalMessageStore;
use wpp::terminal::pager::{fit_line, message_body, wrap_text};
use wpp::terminal::render::{normalize_preview, truncate};
use wpp::whatsapp::models::{Chat, Message, MessageDirection};
use wpp::whatsapp::openwa::models::{ChatSummary, MessageListResponse, RealtimeEnvelope};

fn main() {
  divan::main();
}

const WORDS: &[&str] = &[
  "hola", "che", "mañana", "vamos", "al", "cine", "después", "del", "trabajo",
  "qué", "te", "parece", "🙂", "dale", "nos", "vemos", "a", "las", "ocho",
  "https://example.com/some/very/long/path/that/needs/to/be/wrapped",
];

fn sample_text(seed: usize, words: usize) -> String {
  let mut out = String::new();

  for i in 0..words {
    if i > 0 {
      out.push(if (i + seed) % 17 == 0 { '\n' } else { ' ' });
    }

    out.push_str(WORDS[(i * 7 + seed) % WORDS.len()]);
  }

  out
}

fn sample_chats(count: usize) -> Vec<Chat> {
  (0..count)
    .map(|i| Chat {
      id: format!("549351{:07}@c.us", i),
      name: format!("Contact {} {}", WORDS[i % WORDS.len()], i),
      is_group: i % 9 == 0,
      unread_count: (i % 5) as u32,
      last_message: Some(sample_text(i, 12)),
      timestamp: 1_790_000_000 + i as i64,
    })
    .collect()
}

fn sample_messages(count: usize) -> Vec<Message> {
  (0..count)
    .map(|i| Message {
      id: format!("message-{i}"),
      chat_id: "5493511234567@c.us".to_string(),
      from: "5493511234567@c.us".to_string(),
      to: "5493517654321@c.us".to_string(),
      body: Some(sample_text(i, 5 + i % 40)),
      kind: if i % 13 == 0 { "image" } else { "text" }.to_string(),
      direction: if i % 3 == 0 {
        MessageDirection::Outgoing
      } else {
        MessageDirection::Incoming
      },
      author: None,
      timestamp: Some(1_790_000_000 + i as i64),
      status: "delivered".to_string(),
    })
    .collect()
}

#[divan::bench(args = [100, 1_000, 10_000])]
fn search_chats(bencher: Bencher, count: usize) {
  let chats = sample_chats(count);

  bencher.bench_local(|| {
    let query = black_box("+54 9 351 000-12");
    let query_lower = query.to_lowercase();
    let phone = normalize_phone(query);

    chats
      .iter()
      .filter(|chat| matches_search_query(chat, &query_lower, &phone))
      .count()
  });
}

#[divan::bench(args = [100, 1_000, 10_000])]
fn select_unread_messages(bencher: Bencher, count: usize) {
  let messages = sample_messages(count);

  bencher.bench_local(|| take_unread_messages(black_box(&messages), black_box(count / 2)));
}

#[divan::bench(args = [40, 80, 160])]
fn wrap_message_bodies(bencher: Bencher, width: usize) {
  let messages = sample_messages(200);

  bencher.bench_local(|| {
    messages
      .iter()
      .map(|message| {
        wrap_text(&message_body(message), black_box(width))
          .iter()
          .map(|line| fit_line(line, width))
          .count()
      })
      .sum::<usize>()
  });
}

#[divan::bench]
fn chat_list_previews(bencher: Bencher) {
  let chats = sample_chats(500);

  bencher.bench_local(|| {
    chats
      .iter()
      .map(|chat| {
        let name = truncate(black_box(&chat.name), 30);
        let preview = chat
          .last_message
          .as_deref()
          .map(normalize_preview)
          .map(|text| truncate(&text, 64))
          .unwrap_or_default();

        name.len() + preview.len()
      })
      .sum::<usize>()
  });
}

fn chats_json(count: usize) -> String {
  let chats: Vec<serde_json::Value> = (0..count)
    .map(|i| {
      serde_json::json!({
        "id": format!("549351{:07}@c.us", i),
        "name": format!("Contact {i}"),
        "isGroup": i % 9 == 0,
        "unreadCount": i % 5,
        "lastMessage": sample_text(i, 12),
        "timestamp": 1_790_000_000 + i as i64,
      })
    })
    .collect();

  serde_json::to_string(&chats).unwrap()
}

fn messages_json(count: usize) -> String {
  let messages: Vec<serde_json::Value> = (0..count)
    .map(|i| {
      serde_json::json!({
        "id": format!("message-{i}"),
        "chatId": "5493511234567@c.us",
        "from": "5493511234567@c.us",
        "to": "5493517654321@c.us",
        "body": sample_text(i, 5 + i % 40),
        "type": "text",
        "direction": if i % 3 == 0 { "outgoing" } else { "incoming" },
        "timestamp": 1_790_000_000 + i as i64,
        "status": "delivered",
      })
    })
    .collect();

  serde_json::json!({ "messages": messages }).to_string()
}

#[divan::bench(args = [100, 1_000])]
fn decode_chat_list(bencher: Bencher, count: usize) {
  let json = chats_json(count);

  bencher.bench_local(|| {
    let summaries: Vec<ChatSummary> = serde_json::from_str(black_box(&json)).unwrap();
    summaries.into_iter().map(Chat::from).collect::<Vec<_>>()
  });
}

#[divan::bench(args = [100, 1_000])]
fn decode_message_list(bencher: Bencher, count: usize) {
  let json = messages_json(count);

  bencher.bench_local(|| {
    let response: MessageListResponse = serde_json::from_str(black_box(&json)).unwrap();
    response.messages.len()
  });
}

#[divan::bench]
fn decode_realtime_envelope(bencher: Bencher) {
  let json = serde_json::json!({
    "type": "event",
    "timestamp": "2026-10-02T17:30:00.000Z",
    "payload": {
      "event": "message.received",
      "sessionId": "session-1",
      "data": {
        "id": "message-1",
        "from": "5493511234567@c.us",
        "to": "5493517654321@c.us",
        "chatId": "5493511234567@c.us",
        "body": sample_text(1, 30),
        "type": "text",
        "timestamp": 1790962200,
        "fromMe": false,
        "isGroup": false
      }
    }
  })
  .to_string();

  bencher.bench_local(|| {
    let envelope: RealtimeEnvelope = serde_json::from_str(black_box(&json)).unwrap();
    envelope
  });
}

#[divan::bench(args = [100, 1_000])]
fn sqlite_upsert_messages(bencher: Bencher, count: usize) {
  let messages = sample_messages(count);

  bencher
    .with_inputs(|| LocalMessageStore::in_memory().unwrap())
    .bench_local_values(|mut store| {
      store
        .upsert_messages(black_box("session-1"), black_box(&messages))
        .unwrap()
    });
}
