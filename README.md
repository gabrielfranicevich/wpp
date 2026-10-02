# wpp

> WhatsApp from the terminal — written in Rust.

`wpp` is a lightweight terminal client for WhatsApp.
No GUI, no browser, no Electron.
It talks to WhatsApp through an external [OpenWA](https://github.com/rmyndharis/OpenWA)
server (powered by Baileys) over REST and Socket.IO, while keeping the
OpenWA transport isolated behind a backend-agnostic interface.

---

## Quick start

`wpp` requires a running OpenWA instance. OpenWA is a separate project and is
not bundled with this repository.

```bash
# 1. Start OpenWA separately
#    Default API: http://localhost:2785

# 2. Log in
wpp login          # scan a QR code in the terminal
wpp login +549...  # or pair by phone number

# 3. Use it
wpp list           # see your recent chats
wpp chat Gabriel   # open a conversation
wpp send Gabriel Hey!
```

The OpenWA URL can be changed in the `wpp` configuration file.

---

## Architecture

```text
┌─────────────────────┐
│       WhatsApp      │
└──────────┬──────────┘
           │ Baileys
┌──────────▼──────────┐
│       OpenWA        │
│   external server   │
└──────────┬──────────┘
           │ REST + Socket.IO
┌──────────▼──────────┐
│         wpp         │  ← this repository
│        Rust         │
│                     │
│  CLI (clap)         │
│  multi-session      │
│  lazy pagination    │
│  terminal pager     │
│  local SQLite cache │
└─────────────────────┘
```

`wpp` and `OpenWA` are separate processes.
OpenWA must be running before any command that requires WhatsApp access.

The internal layer stack is:

```text
CLI  →  Application services  →  WhatsApp abstraction  →  OpenWA  →  WhatsApp
```

The CLI has no knowledge of OpenWA-specific HTTP or Socket.IO details.
Those live exclusively in the `whatsapp/openwa` transport layer.

---

## Installation

### Requirements

* **Rust** ≥ 1.78 (2021 edition)
* A running **OpenWA** instance
* Node.js or Docker if required by the OpenWA deployment

### Build from source

```bash
git clone https://github.com/gabrielfranicevich/wpp.git
cd wpp
cargo build --release
# binary: target/release/wpp
```

OpenWA is installed and managed separately from `wpp`.

---

## Configuration

The config file is created automatically on first login.

| Platform | Path                                            |
| -------- | ----------------------------------------------- |
| Linux    | `~/.config/wpp/config.toml`                     |
| Windows  | `%APPDATA%\wpp\config.toml`                     |
| macOS    | `~/Library/Application Support/wpp/config.toml` |

**Example `config.toml`:**

```toml
base_url       = "http://localhost:2785"
api_key        = "your-key"   # optional
active_session = "personal"
```

**Environment variables** take precedence over the config file:

| Variable             | Description         |
| -------------------- | ------------------- |
| `WPP_OPENWA_API_KEY` | OpenWA API key      |
| `OPENWA_API_KEY`     | Alias for the above |

---

## Commands

### `wpp login`

Authenticate with WhatsApp.
Sessions are saved locally and can be reused across runs.

```bash
wpp login                                  # QR code (default)
wpp login --qr                             # QR code (explicit)
wpp login +5491100000000                   # phone pairing code
wpp login +5491100000000 --alias personal  # phone pairing + alias
```

* If the phone number already has a saved session, it is reused automatically.
* After QR authentication, duplicate sessions for the same phone are
  reconciled automatically.
* Multiple aliases can be associated with a single session.

---

### `wpp logout [TARGET]`

Log out a WhatsApp session through OpenWA and remove the local session entry.
All aliases associated with the session are removed.

```bash
wpp logout           # log out the active session
wpp logout personal  # log out a session by alias or phone number
```

---

### `wpp switch [TARGET]`

Manage the active session.
The last used session is automatically restored on the next run.

```bash
wpp switch           # list all saved sessions (with active indicator)
wpp switch personal  # activate a session by alias
wpp switch +549...   # activate a session by phone number
```

---

### `wpp session`

Low-level OpenWA session management.
Shows OpenWA session status and which ones are tracked locally by `wpp`.

```bash
wpp session          # list all OpenWA sessions
wpp session -D <id>  # logout + delete a session
```

`-D` also accepts a local alias or phone number.

If the target refers to a local alias whose OpenWA session no longer exists,
the stale local reference is removed.

---

### `wpp list`

List chats, most recent first. Default limit: 50.

```bash
wpp list                  # recent chats (default: 50)
wpp list --limit 20       # custom limit
wpp list --all            # fetch all chats
wpp list --unread         # fetch chats until unread chats are found
wpp list --filter unread  # filter the selected set to unread chats
```

**Planned:**

```bash
wpp list --dm      # direct messages only
wpp list --groups  # group chats only
```

Interactive chat selection is also planned.

---

### `wpp chat <CONTACT/GROUP>`

Open an interactive terminal pager for a conversation.
The contact can be resolved by exact chat ID, phone number, or
case-insensitive name.

```bash
wpp chat Gabriel
wpp chat 5493511234567
wpp chat 107404297547868@lid

wpp chat Gabriel --unread
wpp chat Gabriel -d
```

**Composable management options:**

```bash
wpp chat <contact> --delete
wpp chat <contact> --block
wpp chat <contact> --unblock
wpp chat <contact> --archive
wpp chat <contact> --unarchive
wpp chat <contact> --pin
wpp chat <contact> --unpin
wpp chat <contact> --mute
wpp chat <contact> --unmute
wpp chat <contact> --mark-read
wpp chat <contact> --mark-unread
```

Display and management options can be combined:

```bash
wpp chat Gabriel --unread --block
```

This shows the unread messages first. After the pager is closed, the
contact is blocked.

Opposing actions cancel each other for each pair:

```bash
wpp chat Gabriel --block --unblock
wpp chat Gabriel --archive --unarchive
wpp chat Gabriel --pin --unpin
wpp chat Gabriel --mute --unmute
wpp chat Gabriel --mark-read --mark-unread
```

Groups cannot be blocked or unblocked.

The chat pager receives new incoming messages in real time through OpenWA
`message.received` events.

Audio and video calls are not exposed as outbound chat actions yet.

**Planned:**

```bash
wpp chat <contact> --call
wpp chat <contact> --video-call
```

Additional interactive chat features are planned, including message
selection, message search, replies, calls, audio, multimedia, polls,
locations, documents, stickers, and contacts.

---

### `wpp search <QUERY>`

Search chats by name or phone number without opening them.

Name matching is case-insensitive and phone matching ignores formatting.
Partial matches are supported.

```bash
wpp search gabriel
wpp search +549
wpp search 351
```

**Planned:** interactive selector inside `wpp search`.

---

### `wpp send <CONTACT> <MESSAGE>`

Send a text message.
The contact is resolved the same way as with `wpp chat`.

```bash
wpp send Gabriel hey
wpp send +5491100000000 "how are you?"
wpp send Gabriel this works with multiple words too
```

**Planned:**

```bash
## --caption <caption> is optional
wpp send <CONTACT> --file <path>

## --caption <caption>, --HD and --GIF are optional
wpp send <CONTACT> --video <path>

## --caption <caption> and --HD are optional
wpp send <CONTACT> --photo <path>

## --animated (true|false) is optional, default is false
wpp send <CONTACT> --sticker <path>

## --name <name> and --address <address> are optional
wpp send <CONTACT> --location <latitude> <longitude>

## --multiple (true|false) is optional, default is false
wpp send <CONTACT> --poll <question> <option1> <option2> ...

## --name <name> is optional, --vcard is optional
wpp send <CONTACT> --contact <phone number>
```

---

### `wpp listen`

Stay running and display incoming messages in real time.

```bash
wpp listen
```

The listener subscribes to OpenWA `message.received` events for the active
session.

Press `Ctrl+C` to stop.

---

### `wpp sync`

Synchronize OpenWA message history into the local SQLite cache.

```bash
wpp sync
wpp sync --limit 100
wpp sync --who "Gabriel,+549..."
```

* Without `--who`, all chats are synchronized.
* `--who` accepts a comma-separated list of exact chat names, phone numbers,
  or chat IDs.
* `--limit` limits the number of messages synchronized per chat.
* Synchronization is idempotent: running it again updates existing messages
  instead of creating duplicates.
* Messages are stored separately per WhatsApp session.

The local cache is stored in the platform's local application-data directory
under `wpp/messages.sqlite`.

---

## Chat pager

`wpp chat` opens a full-screen interactive terminal pager.

### Layout

* Incoming messages → left-aligned
* Outgoing messages → right-aligned
* Consecutive messages from the same sender are visually grouped
* Sender names are shown for incoming messages in group chats
* Each sender gets a deterministic terminal colour
* Long messages are wrapped to the terminal width
* Embedded newlines remain distinguishable within the same message
* The screen redraws only when something changes
* Terminal resize is handled automatically
* Incoming realtime messages appear while the pager is open
* New messages automatically scroll into view when already at the bottom

### History loading

* The pager loads a recent window of messages on open
* Scrolling up past the top triggers lazy loading of older messages
* The first visible message remains at the same screen position after older
  messages are loaded
* `Home` loads all currently available history

### Message composer

Press `Ctrl+Space` inside the pager to open the inline message composer.

| Key            | Action                         |
| -------------- | ------------------------------ |
| `Ctrl+Space`   | Open composer                  |
| `Enter`        | Send message and close         |
| `←` / `→`      | Move cursor                    |
| `Home` / `End` | Jump to start / end of input   |
| `Backspace`    | Delete character before cursor |
| `Delete`       | Delete character after cursor  |
| `Esc`          | Cancel and close composer      |
| `Ctrl+C`       | Does not cancel the composer   |
| `Shift+Enter`  | Insert a new line              |

### Navigation controls

| Key         | Action                                   |
| ----------- | ---------------------------------------- |
| `↑` / `k`   | Scroll up one line / load older messages |
| `↓` / `j`   | Scroll down one line                     |
| `PageUp`    | Scroll one page up                       |
| `PageDown`  | Scroll one page down                     |
| `Home`      | Load all available older messages        |
| `End`       | Jump to the newest loaded messages       |
| `q` / `Esc` | Close the pager                          |
| `Ctrl+C`    | Close the pager                          |

### Pending pager improvements

* Non-blocking `Home` for very large histories through incremental or
  asynchronous loading

---

## Project structure

```text
wpp/
├── Cargo.toml
├── src/
│   ├── main.rs                   # Entry point + command dispatch
│   ├── error.rs                  # Top-level error types
│   │
│   ├── cli/                      # Argument parsing (clap)
│   │   ├── mod.rs                # Command definitions
│   │   ├── login.rs              # QR + phone pairing flow
│   │   ├── logout.rs             # Session logout
│   │   ├── switch.rs             # Session switching + listing
│   │   ├── session.rs            # OpenWA session management
│   │   ├── list.rs               # Chat listing
│   │   ├── chat.rs               # Chat history + management
│   │   ├── search.rs             # Chat search
│   │   ├── send.rs               # Message sending
│   │   ├── listen.rs             # Realtime incoming messages
│   │   └── sync.rs               # Local message synchronization
│   │
│   ├── app/                      # Application logic
│   │   ├── config.rs             # TOML config + session store
│   │   ├── state.rs              # Runtime context
│   │   └── services/
│   │       ├── chats.rs           # Chat listing, filtering, search, management
│   │       ├── chat_resolver.rs   # Contact resolution
│   │       ├── messages.rs        # Lazy history + message sending
│   │       └── sync.rs            # Message synchronization service
│   │
│   ├── storage/                  # Local persistence
│   │   ├── mod.rs
│   │   └── sqlite.rs             # SQLite message cache
│   │
│   ├── whatsapp/                 # WhatsApp abstraction layer
│   │   ├── client.rs             # Backend-agnostic WhatsAppClient
│   │   ├── models.rs             # Domain models
│   │   └── openwa/               # OpenWA transport
│   │       ├── client.rs         # REST + Socket.IO client
│   │       └── models.rs         # OpenWA DTOs
│   │
│   └── terminal/                # Terminal UI
│       ├── pager.rs              # Interactive pager + composer
│       └── render.rs             # Chat list and message rendering
```

OpenWA itself is external to this repository.

---

## Development status

### Implemented

**Session management:**

* QR code authentication rendered in the terminal
* Phone number pairing code
* Session aliases
* Multiple aliases per OpenWA session
* Automatic session reuse when logging in with a known phone number
* Automatic session reconciliation after duplicate QR logins
* Session listing (`wpp switch`, `wpp session`)
* Session switching with active indicator
* Session logout
* OpenWA session deletion with stale-reference cleanup

**Chat listing:**

* List recent chats, most recent first
* Default limit of 50
* Custom limit
* Fetch all chats with internal pagination
* List unread chats
* Filter selected chats to unread only
* Sorting by most recent timestamp

**Chat resolution:**

* Exact chat ID resolution
* Phone-number resolution with digit normalization
* Case-insensitive name resolution
* Ambiguous-match reporting

**Chat pager:**

* Full-screen alternate terminal buffer
* Incoming/outgoing alignment
* Visual grouping of consecutive same-sender messages
* Sender names in group chats
* Deterministic sender colours
* Long-message word wrapping
* Lazy history loading
* Full history loading with `Home`
* Viewport position preservation after loading older messages
* Terminal resize handling
* Inline message composer
* Text sending from the composer
* Multiline message composition with `Shift+Enter`
* `Ctrl+C` preserved inside the composer
* Realtime incoming messages
* Automatic scroll-to-bottom when already at the bottom

**Chat actions:**

* View chat history
* Show unread history
* Delete chats
* Block and unblock contacts
* Archive and unarchive chats
* Pin and unpin chats
* Mute and unmute chats
* Mark chats as read or unread
* Combine display and management options

**Search:**

* Partial name search
* Partial phone-number search
* Case-insensitive name matching
* Digit-normalized phone matching

**Messaging:**

* Send text messages from the command line
* Send text messages from inside the pager

**Realtime:**

* `wpp listen`
* OpenWA `message.received` subscription
* Realtime integration inside the chat pager
* Non-blocking event handling while interacting with the pager

**Local storage:**

* SQLite-backed message cache
* Idempotent message synchronization
* Keyset pagination through the OpenWA persisted-message endpoint
* Per-session message isolation

### Pending

**Chat listing:**

* `wpp list --dm` — direct messages only
* `wpp list --groups` — group chats only
* Interactive chat selector in `wpp list`
* Interactive chat selector in `wpp search`

**Pager:**

* Non-blocking `Home` for very large histories

**Messaging:**

* File and media sending
* Stickers
* Locations
* Polls
* Contacts
* Rich message types

**Calls:**

```bash
wpp call <CONTACT>
wpp call <CONTACT> --video
```

**Chat interaction:**

* Message selection
* Message search inside a conversation
* Replies
* Audio messages
* Multimedia actions
* Additional WhatsApp message types

---

## Roadmap

```text
interactive chat selector
        ↓
wpp list --dm / --groups
        ↓
non-blocking large-history loading
        ↓
rich messaging + additional chat interactions
        ↓
native Rust WhatsApp backend
```

The OpenWA transport remains the current backend until the native Rust
backend is implemented.

---

## Design goals

* Terminal-first, no GUI, no web framework.
* Minimal RAM footprint.
* Composable and scriptable CLI commands.
* Clean separation between CLI, application logic, storage, and transport.
* Backend-agnostic application architecture.
* Lazy loading so large histories do not block the UI.
* Local persistence for synchronized message history.
* Keep OpenWA-specific details isolated from the rest of the application.

---

## License

Not yet defined.
