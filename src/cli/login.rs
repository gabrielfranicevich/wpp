use anyhow::Result;
use crossterm::style::Stylize;
use std::io::Write;

use crate::app::config::{Config, SessionEntry};
use crate::whatsapp::client::OpenWAClient;

/// `wpp login [--phone NUMBER] [--alias NAME]`
///
/// Creates a new session in OpenWA, shows QR or pairing code,
/// waits for authentication, and saves the session to config.
pub async fn run(phone: Option<String>, alias: String) -> Result<()> {
    let mut config = Config::load()?;

    // Don't overwrite an existing session
    if config.sessions.contains_key(&alias) {
        eprintln!(
            "{}",
            format!(
                "Session '{alias}' already exists. Use `wpp switch {alias}` to activate it,\n\
                 or choose a different alias with `wpp login --alias <name>`."
            )
            .yellow()
        );
        return Ok(());
    }

    let client = OpenWAClient::new(&config.base_url);

    // ── 1. Create session in OpenWA ────────────────────────────
    eprintln!("{}", "  Creating session...".dark_grey());
    let session_name = format!("wpp-{}", alias);
    let session = match client.create_session(&session_name).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "{} Could not connect to OpenWA at {}",
                "✗".red().bold(),
                config.base_url
            );
            eprintln!(
                "  {}",
                "Make sure OpenWA is running (e.g. `cd openwa && npm run dev`)".dark_grey()
            );
            return Err(e.into());
        }
    };
    let id = session.id.clone();

    // ── 2. Start the session engine ────────────────────────────
    eprintln!("{}", "  Starting engine...".dark_grey());
    client.start_session(&id).await?;

    // Give the engine a moment to initialise and generate a QR
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // ── 3. Show QR or pairing code ─────────────────────────────
    if let Some(ref phone_number) = phone {
        // Pairing-code flow
        eprintln!("{}", "  Requesting pairing code...".dark_grey());
        let pairing = client.request_pairing_code(&id, phone_number).await?;
        println!();
        println!(
            "  Enter this code in WhatsApp → Linked Devices → Link with phone number:"
        );
        println!();
        println!("    {}", format_pairing_code(&pairing.code).green().bold());
        println!();
    } else {
        // QR-code flow: retry a few times because the engine needs time
        eprintln!("{}", "  Waiting for QR code...".dark_grey());
        let mut qr_data = None;
        for _ in 0..15 {
            match client.get_qr(&id).await {
                Ok(qr) => {
                    qr_data = Some(qr.qr_code);
                    break;
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        let qr_data = qr_data.ok_or_else(|| {
            anyhow::anyhow!(
                "Could not get QR code after 30 s. Is OpenWA running with ENGINE_TYPE=baileys?"
            )
        })?;

        render_qr(&qr_data)?;
        println!();
        println!("  Scan this QR with WhatsApp → Linked Devices → Link a Device");
        println!();
    }

    // ── 4. Poll until authenticated ────────────────────────────
    eprint!("{}", "  Waiting for authentication".dark_grey());
    let mut authenticated = false;

    for _ in 0..90 {
        // ~3 minutes
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Ok(s) = client.get_session(&id).await {
            match s.status.as_str() {
                "ready" => {
                    eprintln!();
                    let who = s
                        .push_name
                        .as_deref()
                        .or(s.phone.as_deref())
                        .unwrap_or("WhatsApp");
                    println!(
                        "  {} Logged in as {}",
                        "✓".green().bold(),
                        who.bold()
                    );
                    if let Some(ref ph) = s.phone {
                        println!("    Phone: {ph}");
                    }

                    // Persist
                    config.sessions.insert(
                        alias.clone(),
                        SessionEntry {
                            id: id.clone(),
                            phone: s.phone.clone(),
                            push_name: s.push_name.clone(),
                        },
                    );
                    config.active_session = Some(alias);
                    config.save()?;

                    authenticated = true;
                    break;
                }
                "failed" => {
                    eprintln!();
                    eprintln!(
                        "  {} Session failed to authenticate.",
                        "✗".red().bold()
                    );
                    break;
                }
                _ => {
                    eprint!(".");
                    std::io::stderr().flush()?;
                }
            }
        }
    }

    if !authenticated {
        eprintln!();
        eprintln!(
            "{}",
            "  Timed out waiting for authentication. Run `wpp login` to try again."
                .yellow()
        );
    }

    Ok(())
}

// ── QR rendering ───────────────────────────────────────────────

/// Render QR code in the terminal using Unicode half-block characters.
///
/// Each printed character encodes two vertical modules, so the QR
/// takes half the lines a naive renderer would need.  Colours are
/// inverted (dark terminals: spaces for dark modules, █ for light)
/// so phone cameras see proper dark-on-light contrast.
fn render_qr(data: &str) -> Result<()> {
    let code = qrcode::QrCode::new(data.as_bytes())?;
    let modules = code.to_colors();
    let w = code.width();

    let quiet = 2; // quiet-zone modules on each side
    let total_w = w + 2 * quiet;

    println!();

    // Top quiet zone (one full-block line)
    print!("    ");
    for _ in 0..total_w {
        print!("██");
    }
    println!();

    // Body — two rows per printed line
    let mut row = 0usize;
    while row < w {
        print!("    ");

        // Left quiet zone
        for _ in 0..quiet {
            print!("██");
        }

        for col in 0..w {
            let top = modules[row * w + col];
            let bot = if row + 1 < w {
                modules[(row + 1) * w + col]
            } else {
                qrcode::Color::Light
            };

            // Inverted: light QR → █ (bright fg), dark QR → space (dark bg)
            match (top, bot) {
                (qrcode::Color::Light, qrcode::Color::Light) => print!("██"),
                (qrcode::Color::Light, qrcode::Color::Dark) => print!("▀▀"),
                (qrcode::Color::Dark, qrcode::Color::Light) => print!("▄▄"),
                (qrcode::Color::Dark, qrcode::Color::Dark) => print!("  "),
            }
        }

        // Right quiet zone
        for _ in 0..quiet {
            print!("██");
        }
        println!();

        row += 2;
    }

    // Bottom quiet zone
    print!("    ");
    for _ in 0..total_w {
        print!("██");
    }
    println!();

    Ok(())
}

/// Format pairing code with a dash in the middle: "ABCD-EFGH"
fn format_pairing_code(code: &str) -> String {
    if code.len() == 8 {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}
