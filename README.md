# wpp

`wpp` is a terminal-based WhatsApp client written in Rust.

It uses [OpenWA](https://github.com/rmyndharis/OpenWA) as its WhatsApp gateway and provides a CLI-oriented interface for managing sessions, chats and messages directly from the terminal.

> **Vibecoded project**
>
> This project is vibecoded: most of the implementation has been developed through iterative interaction with AI coding tools. The code should therefore be treated as an experimental personal project rather than production-grade software.

## ⚠️ Before connecting a phone number

`wpp` depends on **OpenWA**, which is an unofficial, community-maintained WhatsApp gateway.

OpenWA does **not*- use Meta's official WhatsApp Cloud API. It connects to WhatsApp through reverse-engineered clients, including:

- [`whatsapp-web.js`](https://github.com/pedroslopez/whatsapp-web.js)
- [`@whiskeysockets/baileys`](https://github.com/WhiskeySockets/Baileys)

This introduces risks that are outside the control of `wpp`.

### Account restriction and ban risk

There is always a **non-zero risk of WhatsApp restricting or banning an account*- connected through an unofficial client.

No amount of code quality in `wpp` or OpenWA can make that risk disappear.

For that reason:

- Do not connect your primary personal or business number if losing it would be problematic.
- Prefer a dedicated number that you can afford to lose.
- Do not assume that using `wpp` makes automated messaging safe.

### OpenWA engines

OpenWA supports different underlying engines with different trade-offs:

| Engine | General trade-off | Resource usage |
| --- | --- | --- |
| `whatsapp-web.js` | Uses a real headless Chromium browser and generally has a lower-risk profile according to OpenWA's documentation | High RAM usage, roughly 300–500 MB per session |
| `baileys` | Uses the multi-device WebSocket protocol directly and has a higher-risk profile according to OpenWA's documentation | Lower RAM usage, roughly 30–80 MB per session |

These are OpenWA's documented trade-offs, not guarantees.

### Sending messages safely

OpenWA recommends treating automated sending conservatively.

In particular:

- Warm up newly connected numbers instead of immediately performing large batches of automated messaging.
- Avoid cold messaging large numbers of people who have never contacted the account.
- Prefer recipients who have opted in or who already expect the message.
- Pace automated sends instead of attempting high-volume bursts.
- Keep an alternative communication path for authentication-critical or business-critical workflows.
- Be aware that the hosting IP and network environment can also affect account trust.

OpenWA documents additional sending controls such as session pacing, warm-up schedules and limits. See its [Risk Management guide](https://github.com/rmyndharis/OpenWA/blob/main/docs/16-risk-management.md).

### Known WhatsApp platform behaviour

OpenWA also documents behaviour that can originate from WhatsApp's server-side policies rather than from the gateway itself.

Examples include:

- A first message to a completely new contact may succeed at the API level but never arrive.
- A WhatsApp-restricted account cannot simply be unrestricted by OpenWA.
- Some accounts may be unable to complete companion-device linking because WhatsApp requires a passkey step that the supported engines do not implement.

These behaviours are documented by OpenWA and should not automatically be interpreted as bugs in `wpp`.

### Compliance

OpenWA itself recommends using Meta's official WhatsApp Cloud API for deployments where legal, ethical or regulatory compliance is critical.

This includes environments such as:

- healthcare
- finance
- large-scale commercial messaging
- regulated end-user communications
- deployments subject to EU/EEA regulatory requirements

For those use cases, `wpp` should be considered unsuitable unless the risks and compliance requirements have been explicitly evaluated.

See OpenWA's [Risk Management documentation](https://github.com/rmyndharis/OpenWA/blob/main/docs/16-risk-management.md) and [official WhatsApp Cloud API documentation](https://developers.facebook.com/docs/whatsapp/cloud-api).

---

## Requirements

- Rust and Cargo
- A running [OpenWA](https://github.com/rmyndharis/OpenWA) instance
- A WhatsApp account that can be linked to that OpenWA instance

By default, `wpp` expects OpenWA at:

```text
http://localhost:2785
````

The OpenWA API key can be provided through:

```text
WPP_OPENWA_API_KEY
```

or:

```text
OPENWA_API_KEY
```

The repository also contains OpenWA as a Git submodule:

```text
openwa/
```

---

## Usage

### Login

Login using a QR code:

```bash
wpp login
```

Login using a phone pairing code:

```bash
wpp login 5493511234567
```

An optional alias can be assigned to the session:

```bash
wpp login --alias personal
```

Aliases are local identifiers managed by `wpp`.

### Logout

Logout from the active session:

```bash
wpp logout
```

Or specify an alias or phone number:

```bash
wpp logout personal
```

### Sessions

List OpenWA sessions and show which ones are known locally:

```bash
wpp session
```

Delete an OpenWA session:

```bash
wpp session --delete personal
```

### Switch session

List sessions:

```bash
wpp switch
```

Switch the active session:

```bash
wpp switch personal
```

### List chats

List recent chats:

```bash
wpp list
```

The default limit is 50 chats.

Use a custom limit:

```bash
wpp list --limit 10
```

Fetch all chats:

```bash
wpp list --all
```

List unread chats:

```bash
wpp list --unread
```

Filter the selected chats:

```bash
wpp list --limit 10 --filter unread
```

Additional chat-type filters are planned.

### Search

Search chats by name or number:

```bash
wpp search gabriel
```

Search is intended to provide chat discovery independently from opening a conversation.

### Open a chat

Open a chat using its name, phone number or chat ID:

```bash
wpp chat "Gabriel"
```

```bash
wpp chat 5493511234567
```

```bash
wpp chat 107404297547868@lid
```

The resolver currently supports exact identifiers and exact case-insensitive names.

---

## Chat pager

`wpp chat` displays the conversation using an interactive terminal pager.

Current behaviour includes:

- Incoming messages aligned to the left.
- Outgoing messages aligned to the right.
- Consecutive messages from the same sender grouped together.
- Sender names displayed for incoming group messages.
- Different senders receive deterministic terminal colours.
- Each individual WhatsApp message has its own `•` indicator.
- Lines created by a message's internal newline are treated as continuations rather than new messages.
- Long messages are wrapped rather than truncated.
- The screen is redrawn only when something changes to reduce terminal flicker.
- Older history can be loaded incrementally.

### Pager controls

| Key         | Action                            |
| ----------- | --------------------------------- |
| `↑` / `k`   | Scroll up / load older messages   |
| `↓` / `j`   | Scroll down                       |
| `PageUp`    | Scroll one page up                |
| `PageDown`  | Scroll one page down              |
| `Home`      | Load all available older messages |
| `End`       | Go to the newest loaded messages  |
| `q` / `Esc` | Close the pager                   |
| `Ctrl+C`    | Close the pager                   |

---

## Project structure

```text
wpp/
├── .gitignore
├── .gitmodules
├── Cargo.toml
├── Cargo.lock
├── openwa/                    # OpenWA git submodule
│
└── src/
    ├── main.rs                # Application entry point
    ├── error.rs               # Application-level errors
    │
    ├── app/
    │   ├── mod.rs
    │   ├── config.rs          # Local session configuration
    │   ├── state.rs            # Active application state
    │   └── services/
    │       ├── mod.rs
    │       ├── chats.rs       # Chat listing operations
    │       ├── chat_resolver.rs
    │       │                    # Name / number / ID resolution
    │       └── messages.rs    # Lazy message history
    │
    ├── cli/
    │   ├── mod.rs             # Clap CLI definitions
    │   ├── login.rs           # Login flow
    │   ├── logout.rs          # Logout flow
    │   ├── switch.rs          # Session switching
    │   ├── session.rs         # Session management
    │   ├── list.rs            # Chat listing
    │   ├── chat.rs            # Chat history / chat actions
    │   └── search.rs          # Chat search
    │
    ├── terminal/
    │   ├── mod.rs
    │   ├── pager.rs           # Interactive message pager
    │   └── render.rs          # Terminal rendering helpers
    │
    └── whatsapp/
        ├── mod.rs
        ├── client.rs          # WhatsApp abstraction
        ├── models.rs          # Application models
        │
        └── openwa/
            ├── mod.rs
            ├── client.rs      # OpenWA transport
            └── models.rs      # OpenWA API models
```

The project is intentionally divided into layers:

```text
CLI
 │
 ▼
Application services
 │
 ▼
WhatsApp abstraction
 │
 ▼
OpenWA
 │
 ▼
WhatsApp
```

The CLI should not need to know OpenWA-specific HTTP details. Those details belong in the WhatsApp/OpenWA layer.

---

## Development status

The project is being developed incrementally, one feature at a time.

### Implemented

- Session login
- QR authentication
- Phone pairing
- Session aliases
- Multiple aliases per session
- Session reuse by phone number
- Session reconciliation after duplicate QR logins
- Session listing
- Session deletion
- Session switching
- Chat listing
- Chat resolution
- Exact name / number / ID resolution
- Lazy message history loading
- Interactive terminal pager
- Unread chat listing
- Chat search command skeleton
- Message sending command skeleton
- Realtime listener command skeleton
- Local synchronization command skeleton

### Still pending

The following functionality is planned but not yet fully implemented:

#### Pager

- Preserve the exact viewport position when older messages are loaded.
- Avoid blocking the UI when `Home` causes a large history to be loaded.
- Evaluate incremental or asynchronous history loading for very large conversations.

#### Chat listing

A chat selector in wpp list and wpp search

Additional filters:

```bash
wpp list --dm
```

Only direct conversations.

```bash
wpp list --groups
```

Only group conversations.

#### Message sending

Send a message directly from the command line:

```bash
wpp send <contact> <message>
```

Interactive message composition from inside `wpp chat` is also planned.

#### Chat management

Additional actions are planned:

```bash
wpp chat --delete <contact>
wpp chat --block <contact>
wpp chat --unblock <contact>
wpp chat --archive <contact>
wpp chat --unarchive <contact>
wpp chat --pin <contact>
wpp chat --unpin <contact>
wpp chat --mute <contact>
wpp chat --unmute <contact>
```

#### Realtime events

`wpp listen` will keep running and display incoming activity in real time, including unread chat updates.

#### Local synchronization

`wpp sync` will persist messages locally.

Planned options include:

```bash
wpp sync --limit 100
```

and:

```bash
wpp sync --who "Gabriel,5493511234567"
```

The local synchronization layer is intended to eventually provide a persistent message cache rather than relying exclusively on OpenWA history requests.

---

## Roadmap

The planned implementation order is approximately:

```text
lazy chat history pager
        ↓
chat resolver
        ↓
unread chat history
        ↓
chat search
        ↓
message sending
        ↓
chat deletion
        ↓
chat blocking
        ↓
chat archiving
        ↓
chat pinning
        ↓
chat muting
        ↓
realtime message listener
        ↓
local message synchronization
```

The roadmap is intentionally incremental so that each feature can be implemented and tested independently.

---

## Design goals

`wpp` is primarily an experimental terminal client rather than a replacement for WhatsApp's official applications.

The main goals are:

- Terminal-first interaction.
- Minimal and composable CLI commands.
- Clear separation between application logic and OpenWA transport.
- Lazy loading for large chat histories.
- Local session management.
- Eventually, local message persistence and realtime updates.

The project currently prioritizes functionality and iteration speed over production hardening.

---

## License

This project does not currently define a public license.
