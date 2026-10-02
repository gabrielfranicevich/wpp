# wpp

> WhatsApp from the terminal — written in Rust.

`wpp` is a lightweight terminal client for WhatsApp.
No GUI, no browser, no Electron.
It talks to WhatsApp through an [OpenWA](https://openwa.js.org/) server
(powered by Baileys) over REST, and is designed so that the OpenWA transport
layer can eventually be replaced by a native Rust implementation.

---

## Quick start

```bash
# 1. Start OpenWA (default port: 2785)
cd openwa && npm start

# 2. Log in
wpp login          # scan a QR code in the terminal
wpp login +549...  # or pair by phone number

# 3. Use it
wpp list           # see your recent chats
wpp chat Gabriel   # open a conversation
wpp send Gabriel Hey!
```

---

## Architecture

```text
┌─────────────────────┐
│       WhatsApp      │
└──────────┬──────────┘
           │ Baileys
┌──────────▼──────────┐
│        OpenWA       │
│  connection+session │
└──────────┬──────────┘
           │ REST
┌──────────▼──────────┐
│         wpp         │  ← this repository
│        Rust         │
│                     │
│  CLI (clap)         │
│  multi-session      │
│  lazy pagination    │
│  terminal pager     │
└─────────────────────┘
```

`wpp` and `openwa` are two separate processes.
OpenWA must be running before any `wpp` command is issued.

The internal layer stack is:

```text
CLI  →  Application services  →  WhatsApp abstraction  →  OpenWA  →  WhatsApp
```

The CLI has no knowledge of OpenWA-specific HTTP details —
those live exclusively in the `whatsapp/openwa` layer.

---

## Installation

### Requirements

- **Rust** ≥ 1.78 (2021 edition)
- **Node.js** (to run OpenWA)
- OpenWA listening on `http://localhost:2785` (configurable)

### Build from source

```bash
git clone --recurse-submodules https://github.com/<user>/wpp.git
cd wpp
cargo build --release
# binary: target/release/wpp
```

---

## Configuration

The config file is created automatically on first login.

| Platform | Path                                            |
|----------|-------------------------------------------------|
| Linux    | `~/.config/wpp/config.toml`                     |
| Windows  | `%APPDATA%\wpp\config.toml`                     |
| macOS    | `~/Library/Application Support/wpp/config.toml` |

**Example `config.toml`:**

```toml
base_url       = "http://localhost:2785"
api_key        = "your-key"   # optional
active_session = "personal"
```

**Environment variables** (take precedence over the config file):

| Variable             | Description         |
|----------------------|---------------------|
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

- If the phone number already has a saved session, it is reused automatically.
- After QR authentication,
  duplicate sessions for the same phone are reconciled automatically.
- Multiple aliases can be associated with a single session.

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
wpp session          # list all OpenWA sessions (name, phone, ID, status, wpp aliases)
wpp session -D <id>  # logout + delete a session; also accepts alias or phone number
```

If the target refers to a local alias whose OpenWA session no longer exists,
the stale local reference is removed.

---

### `wpp list`

List chats, most recent first. Default limit: 50.

```bash
wpp list                  # recent chats (default: 50)
wpp list --limit 20       # custom limit
wpp list --all            # fetch all chats (paginated internally)
wpp list --unread         # fetch chats until all unread ones are found
wpp list --filter unread  # apply unread filter to the selected set
```

**Planned:**

```bash
wpp list --dm      # direct messages only
wpp list --groups  # group chats only
```

Interactive chat selector inside `wpp list` is also planned.

---

### `wpp chat <CONTACT/GROUP>`

Open an interactive terminal pager for a conversation. Accepts name,
phone number, or raw WhatsApp chat ID.

```bash
wpp chat Gabriel
wpp chat 5493511234567
wpp chat 107404297547868@lid

wpp chat Gabriel --unread   # show only unread messages, oldest-first
wpp chat Gabriel -d         # delete the chat
```

Resolution supports exact identifiers and case-insensitive name matching.

**Composable options:**

Chat options are composable. --unread is a display mode, while management
options perform actions on the resolved chat.

```bash
wpp chat <contact> --delete
wpp chat <contact> --block
wpp chat <contact> --unblock
```

**Display and management options can be combined:**

```bash
wpp chat Gabriel --unread --block
```

This shows the unread messages first.
After the pager is closed, the contact is blocked.

**Multiple management actions can also be combined:**

```bash
wpp chat Gabriel --delete --block
```

This blocks the contact and then deletes the chat.

**Opposing actions cancel each other:**

```bash
wpp chat Gabriel --block --unblock
```

This is a no-op for the block/unblock actions.

**Current management options:**

```bash
wpp chat <contact> --delete
wpp chat <contact> --block
wpp chat <contact> --unblock
```

Groups cannot be blocked or unblocked.

**Planned:**

compatible options should be stackable

```bash
wpp chat <contact> --block      # blocks a contact
wpp chat <contact> --unblock    # unblocks a contact
wpp chat <contact> --archive    # archives the chat
wpp chat <contact> --unarchive  # unarchives the chat
wpp chat <contact> --pin        # pins the chat
wpp chat <contact> --unpin      # unpins the chat
wpp chat <contact> --mute       # mutes the chat
wpp chat <contact> --unmute     # unmutes the chat
wpp chat <contact> --call       # calls a contact (audio call)
wpp chat <contact> --video-call # calls a contact (video call)
wpp chat <contact> --mark-read  # marks the chat as read
wpp chat <contact> --mark-unread  # marks the chat as unread
```

In the `wpp chat <contact>` tui:
  chat should update real time
  the options to select messages, look for a message, reply a specific message,
  call, video call, send audio, erase a message, send multimedia, polls,
  location, documents, stickers, contacts

---

### `wpp search <QUERY>`

Search chats by name or phone number without opening them.
Matching is case-insensitive for names and digit-normalized for phone numbers
(partial matches supported).

```bash
wpp search gabriel
wpp search +549
wpp search 351
```

**Planned:** interactive selector inside `wpp search`.

---

### `wpp send <CONTACT> <MESSAGE>`

Send a text message. The contact is resolved the same way as `wpp chat`.

```bash
wpp send Gabriel hey
wpp send +5491100000000 "how are you?"
wpp send Gabriel this works with multiple words too
```

**Planned:**

```bash
## --caption <caption> is optional (photos or videos are sent as "document")
wpp send <CONTACT> --file <path>
## --caption <caption>, --HD, --GIF are optional
wpp send <CONTACT> --video <path>
## --caption <caption>, --HD are optional
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

### `wpp call` *(planned)*

```bash
wpp call <CONTACT>  # audio call
wpp call <CONTACT> --video  # video call
```

---

### `wpp listen` *(planned)*

Stay running and display incoming messages and unread-chat updates in real time.

---

### `wpp sync` *(planned)*

Persist messages locally to a cache.

```bash
wpp sync                          # sync all chats
wpp sync --limit 100              # max messages per chat
wpp sync --who "Gabriel,+549..."  # only sync specific contacts
```

---

## Chat pager

`wpp chat` opens a full-screen interactive terminal pager.

### Layout

- Incoming messages → left-aligned
- Outgoing messages → right-aligned
- Consecutive messages from the same sender are visually grouped
- Sender names shown for incoming messages in group chats
- Each sender gets a deterministic terminal colour
- Long messages are wrapped to terminal width, never truncated
- Internal newlines within a message are treated as continuations, not new bubbles
- The screen redraws only when something changes (no unnecessary flicker)
- Terminal resize is handled automatically

### History loading

- The pager loads a recent window of messages on open
- Scrolling up past the top triggers lazy loading of older messages
- The viewport position is approximately preserved after loading older messages
- `Home` loads the full available history

### Message composer

Press `Ctrl+Space` inside the pager to open the inline message composer.

| Key               | Action                        |
|-------------------|-------------------------------|
| `Ctrl+Space`      | Open composer                 |
| `Enter`           | Send message and close        |
| `←` / `→`         | Move cursor                   |
| `Home` / `End`    | Jump to start / end of input  |
| `Backspace`       | Delete character before cursor|
| `Delete`          | Delete character after cursor |
| `Esc`             | Cancel and close composer     |
| `Ctrl+C`          | Does not cancel the composer  |
| `Shift+Enter`     | Insert new line               |

### Navigation controls

| Key           | Action                              |
|---------------|-------------------------------------|
| `↑` / `k`     | Scroll up one line / load older     |
| `↓` / `j`     | Scroll down one line                |
| `PageUp`      | Scroll one page up                  |
| `PageDown`    | Scroll one page down                |
| `Home`        | Load all available older messages   |
| `End`         | Jump to the newest loaded messages  |
| `q` / `Esc`   | Close the pager                     |
| `Ctrl+C`      | Close the pager                     |

### Pending pager improvements

- Preserve the exact viewport position when older messages are loaded
  (currently approximated).
- Non-blocking `Home` for very large histories (incremental or async loading).

---

## Project structure

```text
wpp/
├── Cargo.toml
├── openwa/                     # OpenWA git submodule
└── src/
    ├── main.rs                 # Entry point + command dispatch
    ├── error.rs                # Top-level error types
    │
    ├── cli/                    # Argument parsing (clap)
    │   ├── mod.rs              # Command definitions
    │   ├── login.rs            # QR + phone pairing flow
    │   ├── logout.rs           # Session logout
    │   ├── switch.rs           # Session switching + listing
    │   ├── session.rs          # OpenWA session management
    │   ├── list.rs             # Chat listing
    │   ├── chat.rs             # Chat history + delete
    │   ├── search.rs           # Chat search
    │   └── send.rs             # Message sending
    │
    ├── app/                    # Application logic
    │   ├── config.rs           # TOML config + session store
    │   ├── state.rs            # Runtime context (WhatsApp client + active session)
    │   └── services/
    │       ├── chats.rs        # Chat listing, filtering, search, delete
    │       ├── chat_resolver.rs  # Contact resolution (name / phone / ID)
    │       └── messages.rs     # Lazy message history + send
    │
    ├── whatsapp/               # WhatsApp abstraction layer
    │   ├── client.rs           # Backend-agnostic WhatsAppClient
    │   ├── models.rs           # Domain models (Chat, Message, Session, …)
    │   └── openwa/             # OpenWA REST transport
    │       ├── client.rs
    │       └── models.rs
    │
    └── terminal/               # Terminal UI
        ├── pager.rs            # Full-screen interactive pager + composer
        └── render.rs           # Chat list and message rendering helpers
```

---

## Development status

### Implemented

**Session management:**

- QR code authentication (rendered in the terminal)
- Phone number pairing code
- Session aliases (multiple aliases per session)
- Automatic session reuse when logging in with a known phone number
- Automatic session reconciliation after duplicate QR logins (same phone → merged)
- Session listing (`wpp switch`, `wpp session`)
- Session switching with active indicator
- Session logout (removes all associated local aliases)
- OpenWA session deletion with stale-reference cleanup

**Chat listing:**

- List recent chats (most recent first, default limit 50)
- Custom limit (`--limit`)
- Fetch all chats with internal pagination (`--all`)
- List only unread chats (`--unread`)
- Filter a selected set to unread only (`--filter unread`)
- Chats sorted by most recent timestamp

**Chat resolution:**

- Resolve contact by exact chat ID (e.g. `5493511234567@c.us`)
- Resolve by phone number (digit-normalized, formatting ignored)
- Resolve by case-insensitive name match

**Chat pager:**

- Full-screen alternate terminal buffer
- Incoming/outgoing message alignment (left/right)
- Visual grouping of consecutive same-sender messages
- Sender name display for group incoming messages
- Deterministic per-sender terminal colour
- Long-message word wrapping
- Lazy history loading on scroll-up
- Load all available history (`Home`)
- Terminal resize handling
- Inline message composer (`Ctrl+Space`)
- Send message from composer (`Enter`)
- Multiline message composition with (`Shift+Enter`)
- `Ctrl+C` does not cancel the composer

**Chat actions:**

- View full chat history (`wpp chat <contact>`)
- View only unread messages, oldest-first (`wpp chat <contact> --unread`)
- Delete a chat (`wpp chat <contact> -d`)
- Block a contact (`wpp chat <contact> --block`)
- Unblock a contact (`wpp chat <contact> --unblock`)
- Combine display and management options

**Search:**

- Search by partial name (case-insensitive)
- Search by partial or formatted phone number

**Messaging:**

- Send text messages from the command line (`wpp send`)
- Send text messages from inside the pager (composer)

### Pending

**Chat listing:**

- `wpp list --dm` — direct messages only
- `wpp list --groups` — group chats only
- Interactive chat selector in `wpp list` and `wpp search`

**Chat management:**

```bash
wpp chat <contact> --archive
wpp chat <contact> --unarchive
wpp chat <contact> --pin
wpp chat <contact> --unpin
wpp chat <contact> --mute
wpp chat <contact> --unmute
wpp chat <contact> --call
wpp chat <contact> --video-call
wpp chat <contact> --mark-read
wpp chat <contact> --mark-unread
```

**Pager:**

- Exact viewport position preservation when older messages are loaded
- Non-blocking `Home` for very large histories

**Realtime:**

- `wpp listen` —
  stay running, display incoming messages and unread updates in real time

**Local sync:**

- `wpp sync` — persist messages locally (`--limit`, `--who`)

---

## Roadmap

```text
interactive chat selector (wpp list / wpp search)
        ↓
wpp list --dm / --groups
        ↓
chat management (block / archive / pin / mute)
        ↓
pager viewport fix + async Home
        ↓
wpp listen (realtime events)
        ↓
wpp sync (local message cache)
        ↓
native Rust backend (no OpenWA)
```

---

## Design goals

- Terminal-first, no GUI, no web framework.
- Minimal RAM footprint.
- Composable and scriptable CLI commands.
- Clean separation between CLI, application logic, and transport.
- Backend-agnostic:
  the OpenWA layer can be swapped without touching the rest of the code.
- Lazy loading so large histories never block the UI.

---

## License

Not yet defined.
