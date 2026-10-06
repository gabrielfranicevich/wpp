# wpp

> WhatsApp from the terminal — written in Rust.

`wpp` is a lightweight terminal client for WhatsApp.

No GUI, no browser, no Electron, and no external WhatsApp server are required.
WhatsApp connectivity, authentication, protocol state, chat storage,
and realtime events are handled directly by the native Rust client.

* * *

## ⚠️ Before connecting a phone number

`wpp` depends on [**whatsapp-rust**](https://github.com/oxidezap/whatsapp-rust),
which is an unofficial, community-maintained WhatsApp gateway.

whatsapp-rust does **not* use Meta's official WhatsApp Cloud API.
It connects to WhatsApp through a reverse-engineered client

This introduces risks that are outside the control of `wpp`.

### Account restriction and ban risk

There is always a **non-zero risk of WhatsApp restricting or banning an account**
connected through an unofficial client.

No amount of code quality in `wpp` or whatsapp-rust can make that risk disappear.

For that reason:

* Do not connect your primary personal or business number
  if losing it would be problematic.
* Prefer a dedicated number that you can afford to lose.
* Do not assume that using `wpp` makes automated messaging safe.

* * *

## Quick start

`wpp` is distributed as a single executable.

```bash
# Authenticate with QR code
wpp login

# Or use a phone pairing code
wpp login +5491100000000

# List recent chats
wpp list

# Open a conversation
wpp chat Gabriel

# Send a message
wpp send Gabriel "Hello from the terminal"
```

Once authenticated, the native session is persisted locally and reused automatically.

* * *

## Installation

### From source

Clone the repository and build the release binary:

```bash
git clone https://github.com/gabrielfranicevich/wpp.git
cd wpp
cargo build --release
```

The executable is generated at:

```text
target/release/wpp
```

It can be copied to any directory in `PATH`.

### With Cargo

```bash
cargo install --path .
```

This installs the `wpp` executable into Cargo’s binary directory.

### Runtime requirements

No separate service is required.

`wpp` does not require:

* Node.js
* Docker
* a browser session
* a locally running WhatsApp server
* a REST API server
* a Socket.IO server

The executable connects directly to WhatsApp through the native Rust stack.

* * *

## Architecture

The application is structured in layers:

```text
┌─────────────────────┐
│       WhatsApp      │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│    whatsapp-rust    │
│ native protocol     │
│ authentication      │
│ realtime events     │
│ persistence         │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│    NativeClient     │
│ native backend      │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│   WhatsAppClient    │
│ application         │
│ abstraction         │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│ Application services│
│ chat / messages /   │
│ sync / resolution   │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│         CLI         │
│        clap         │
└─────────────────────┘
```

The application layer depends on `WhatsAppClient`, not directly on `whatsapp-rust`.

This keeps WhatsApp protocol details isolated from:

* CLI commands
* chat resolution
* message services
* local synchronization
* terminal rendering

Realtime events follow the same boundary:

```text
whatsapp-rust event
        ↓
NativeRealtimeListener
        ↓
RealtimeListener
        ↓
wpp listen / chat pager
```

* * *

## Authentication

`wpp login` supports both QR authentication and phone pairing.

```bash
wpp login
wpp login --qr

wpp login +5491100000000
wpp login --phone +5491100000000

wpp login +5491100000000 --alias personal
```

The phone number is normalized before session lookup.

When a saved session already belongs to that number, `wpp` reuses its native storage
instead of creating another session.

Without an explicit alias, `wpp` generates one from the authenticated phone number.

Multiple aliases can point to the same WhatsApp session.

* * *

## Storage

`wpp` uses two separate SQLite databases.

### Native WhatsApp storage

Each WhatsApp session has its own native database:

```text
<data-local>/wpp/whatsapp/<session-id>.sqlite
```

This storage belongs to the native `whatsapp-rust` stack and contains authentication,
protocol, and native chat state.

### Local message cache

`wpp sync` maintains a separate application-level cache:

```text
<data-local>/wpp/messages.sqlite
```

Messages are isolated by WhatsApp session, using `(session_id, message_id)` as the
logical key.

The two stores are intentionally independent:

```text
<data-local>/wpp/
├── messages.sqlite
└── whatsapp/
    ├── <session-1>.sqlite
    ├── <session-2>.sqlite
    └── ...
```

### Configuration

Session aliases and the active session are stored separately from both databases:

```text
<config-dir>/wpp/config.toml
```

Typical platform locations are:

| Platform | Configuration | Native/application data |
| --- | --- | --- |
| Linux | `~/.config/wpp/config.toml` | `~/.local/share/wpp/` |
| Windows | `%APPDATA%\wpp\config.toml` | `%LOCALAPPDATA%\wpp\` |
| macOS | `~/Library/Application Support/wpp/config.toml` | `~/Library/Application Support/wpp/` |

The configuration file contains session metadata such as aliases, phone numbers,
profile names, and the active session.

It does not contain a remote server URL or API credentials.

* * *

## Sessions

### `wpp switch [TARGET]`

List saved sessions or select the active one.

```bash
wpp switch
wpp switch personal
wpp switch +5491100000000
```

The selected session remains active across subsequent commands and program executions.

### `wpp logout [TARGET]`

Log out a saved session and remove its native storage.

```bash
wpp logout
wpp logout personal
wpp logout +5491100000000
```

All aliases associated with the session are removed.

### `wpp session`

Inspect or delete saved native sessions.

```bash
wpp session
wpp session -D personal
wpp session -D +5491100000000
wpp session -D <session-id>
```

Deleting a session removes both its local metadata and native SQLite storage.

* * *

## Commands

### `wpp login`

Authenticate a WhatsApp account.

```bash
wpp login
wpp login --qr

wpp login +5491100000000
wpp login --phone +5491100000000

wpp login +5491100000000 --alias personal
```

QR authentication is the default when no phone number is provided.

* * *

### `wpp logout [TARGET]`

Log out and remove a saved session.

```bash
wpp logout
wpp logout personal
```

* * *

### `wpp switch [TARGET]`

List sessions or change the active session.

```bash
wpp switch
wpp switch personal
wpp switch +5491100000000
```

* * *

### `wpp session`

List saved sessions or delete one.

```bash
wpp session
wpp session -D <TARGET>
```

* * *

### `wpp list`

List chats, most recent first.

The default limit is 50 chats.

```bash
wpp list
wpp list --limit 20
wpp list --all
wpp list --unread
wpp list --filter unread
```

`--unread` selects unread chats while `--filter unread`
filters the selected chat set.

* * *

### `wpp chat <CONTACT/GROUP>`

Open an interactive conversation pager.

The target can be resolved by:

* exact chat ID
* phone number
* case-insensitive name

Examples:

```bash
wpp chat Gabriel
wpp chat 5493511234567
wpp chat 107404297547868@lid
```

Unread mode:

```bash
wpp chat Gabriel --unread
```

Chat management actions:

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

Display and management actions can be combined:

```bash
wpp chat Gabriel --unread --block
```

Opposing operations cancel each other:

```bash
wpp chat Gabriel --block --unblock
wpp chat Gabriel --archive --unarchive
wpp chat Gabriel --pin --unpin
wpp chat Gabriel --mute --unmute
wpp chat Gabriel --mark-read --mark-unread
```

Groups cannot be blocked or unblocked.

While the pager is open, new messages are received through the native realtime event
stream.

* * *

### `wpp search <QUERY>`

Search chats without opening them.

Name searches are case-insensitive.
Phone searches ignore formatting and support partial matches.

```bash
wpp search gabriel
wpp search +549
wpp search 351
```

* * *

### `wpp send <CONTACT> <MESSAGE>`

Send a text message.

```bash
wpp send Gabriel hey

wpp send +5491100000000 "how are you?"

wpp send Gabriel this message contains multiple words
```

The contact resolver is shared with `wpp chat`.

* * *

### `wpp listen`

Listen for incoming messages in real time.

```bash
wpp listen
```

The command subscribes to the native WhatsApp event stream for the active session.

Example output:

```text
Listening for incoming messages. Press Ctrl+C to stop.
[2026-10-06T18:42:31Z] 5493511234567@c.us → 5493517654321@c.us: Hello
```

Press `Ctrl+C` to stop.

* * *

### `wpp sync`

Synchronize native message history into the local SQLite cache.

```bash
wpp sync
wpp sync --limit 100
wpp sync --who "Gabriel,+5493511234567"
```

Behavior:

* Without `--who`, all available chats are synchronized.
* `--who` accepts a comma-separated list of exact chat names, phone numbers,
  or chat IDs.
* `--limit` limits the number of messages synchronized per chat.
* Synchronization is idempotent.
* Messages are isolated by WhatsApp session.

The command writes to:

```text
<data-local>/wpp/messages.sqlite
```

* * *

## Chat pager

`wpp chat` opens a full-screen interactive terminal pager.

### Layout

* Incoming messages are left-aligned.
* Outgoing messages are right-aligned.
* Consecutive messages from the same sender are visually grouped.
* Sender names are displayed for incoming messages in group chats.
* Each sender receives a deterministic terminal colour.
* Long messages wrap to the terminal width.
* Embedded newlines remain distinguishable.
* Terminal resize is handled automatically.
* Incoming realtime messages appear while the pager is open.
* New messages automatically scroll into view when already at the bottom.

### History loading

The pager initially loads a recent window.

Scrolling above the loaded history triggers lazy loading of older messages.

The current viewport position is preserved when older messages are inserted.

`Home` loads all currently available history.

### Message composer

Press `Ctrl+Space` inside the pager to open the inline composer.

| Key | Action |
| --- | --- |
| `Ctrl+Space` | Open composer |
| `Enter` | Send message and close |
| `←` / `→` | Move cursor |
| `Home` / `End` | Jump to start / end |
| `Backspace` | Delete previous character |
| `Delete` | Delete next character |
| `Esc` | Cancel and close |
| `Ctrl+C` | Remains inside the composer |
| `Shift+Enter` | Insert a new line |

### Navigation

| Key | Action |
| --- | --- |
| `↑` / `k` | Scroll up / load older messages |
| `↓` / `j` | Scroll down |
| `PageUp` | Scroll one page up |
| `PageDown` | Scroll one page down |
| `Home` | Load all available older messages |
| `End` | Jump to newest loaded messages |
| `q` / `Esc` | Close pager |
| `Ctrl+C` | Close pager |

* * *

## Project structure

```text
wpp/
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
└── src/
    ├── main.rs
    ├── error.rs
    │
    ├── cli/
    │   ├── mod.rs
    │   ├── login.rs
    │   ├── logout.rs
    │   ├── switch.rs
    │   ├── session.rs
    │   ├── list.rs
    │   ├── chat.rs
    │   ├── search.rs
    │   ├── send.rs
    │   ├── listen.rs
    │   └── sync.rs
    │
    ├── app/
    │   ├── config.rs
    │   ├── state.rs
    │   └── services/
    │       ├── chats.rs
    │       ├── chat_resolver.rs
    │       ├── messages.rs
    │       └── sync.rs
    │
    ├── storage/
    │   ├── mod.rs
    │   └── sqlite.rs
    │
    ├── terminal/
    │   ├── mod.rs
    │   ├── pager.rs
    │   └── render.rs
    │
    └── whatsapp/
        ├── mod.rs
        ├── client.rs
        ├── models.rs
        └── native/
            ├── mod.rs
            └── client.rs
```

The `whatsapp/native` module is the only WhatsApp transport implementation.

* * *

## Development status

### Implemented

**Authentication and sessions:**

* QR authentication
* Phone pairing code authentication
* Persistent native WhatsApp sessions
* Automatic session reuse by phone number
* Session aliases
* Multiple aliases per session
* Automatic alias generation
* Active session selection
* Session logout
* Native session deletion
* Native session storage cleanup

**Chat listing and resolution:**

* Recent chat listing
* Default limit of 50
* Custom limits
* Full chat listing
* Unread chat selection
* Unread filtering
* Most-recent-first sorting
* Exact chat ID resolution
* Exact phone-number resolution
* Case-insensitive name resolution
* Partial chat search
* Ambiguous-match reporting

**Chat management:**

* Delete chats
* Block and unblock contacts
* Archive and unarchive chats
* Pin and unpin chats
* Mute and unmute chats
* Mark chats as read or unread
* Combining display and management actions

**Messaging:**

* Command-line text sending
* Text sending from the pager
* Native message history
* Native realtime incoming messages

**Realtime:**

* `wpp listen`
* Native realtime event subscription
* Conversion of native message events to the application `RealtimeEvent`
* Realtime integration inside the chat pager
* Non-blocking realtime handling while interacting with the pager
* Duplicate event suppression in the pager

**Local synchronization:**

* Native message-history synchronization
* SQLite-backed application cache
* Idempotent message synchronization
* Keyset pagination
* Per-session message isolation

**Terminal UI:**

* Full-screen pager
* Incoming/outgoing alignment
* Sender grouping
* Group-chat sender names
* Deterministic sender colours
* Long-message wrapping
* Lazy history loading
* Full-history loading
* Viewport preservation
* Terminal resize handling
* Inline text composer
* Multiline message composition
* Realtime updates while browsing

### Pending

**Chat listing:**

* `wpp list --dm`
* `wpp list --groups`
* Interactive chat selector

**Pager:**

* Non-blocking loading of very large histories

**Messaging:**

* File and media sending
* Photos and videos
* Stickers
* Locations
* Polls
* Contacts
* Rich message types

**Chat interaction:**

* Message selection
* Message search inside a conversation
* Replies
* Audio messages
* Multimedia actions

**Calls:**

```bash
wpp call <CONTACT>
wpp call <CONTACT> --video
```

* * *

## Design goals

* Terminal-first interface.
* Single executable.
* No external WhatsApp service.
* Native Rust WhatsApp transport.
* Clear separation between CLI, application logic, storage, and protocol code.
* Application services independent from `whatsapp-rust`.
* Persistent native session state.
* Separate application-level message cache.
* Lazy loading for large histories.
* Composable and scriptable commands.
* Minimal unnecessary runtime infrastructure.

* * *

## License

MIT License.
