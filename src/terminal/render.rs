use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossterm::style::Stylize;

use crate::whatsapp::models::Chat;

const NAME_WIDTH: usize = 30;
const PREVIEW_WIDTH: usize = 64;
const QR_QUIET_ZONE: usize = 2;

pub fn render_chats(chats: &[Chat]) {
    if chats.is_empty() {
        println!(" No chats found.");
        return;
    }

    for chat in chats {
        let marker = if chat.unread_count > 0 {
            "●".green().bold().to_string()
        } else {
            " ".to_string()
        };

        let kind = if chat.is_group { "G" } else { " " };

        let name = truncate(
            if chat.name.trim().is_empty() {
                &chat.id
            } else {
                &chat.name
            },
            NAME_WIDTH,
        );

        let preview = chat
            .last_message
            .as_deref()
            .map(normalize_preview)
            .map(|text| truncate(&text, PREVIEW_WIDTH))
            .unwrap_or_default();

        let age = format_age(chat.timestamp);

        let unread = if chat.unread_count > 0 {
            chat.unread_count.to_string().green().bold().to_string()
        } else {
            String::new()
        };

        println!(
            " {marker} {kind} {name:<NAME_WIDTH$} {age:>4} {unread:>4} {preview}",
            NAME_WIDTH = NAME_WIDTH,
        );
    }
}

/// Decode the QR image returned by OpenWA, recover its payload,
/// and generate a fresh QR matrix from that payload.
///
/// We intentionally never render the original image pixels.
/// This avoids losing QR information through image scaling or
/// terminal pixel conversion.
pub fn render_qr(data: &str) -> Result<()> {
    let b64 = data
        .strip_prefix("data:image/png;base64,")
        .or_else(|| data.strip_prefix("data:image/jpeg;base64,"))
        .or_else(|| data.strip_prefix("data:image/jpg;base64,"))
        .context("OpenWA returned an unsupported QR image format")?;

    use base64::Engine as _;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .context("failed to decode QR image base64")?;

    let image = image::load_from_memory(&bytes)
        .context("failed to decode QR image")?
        .into_luma8();

    let mut decoder = quircs::Quirc::default();

    let codes = decoder.identify(image.width() as usize, image.height() as usize, &image);

    let mut payload = None;

    for code in codes {
        let code = code.context("failed to detect QR code")?;

        let decoded = code.decode().context("failed to decode QR code")?;

        let text = String::from_utf8(decoded.payload).context("QR payload is not valid UTF-8")?;

        payload = Some(text);
        break;
    }

    let payload = payload.context("no readable QR code found in OpenWA image")?;

    render_qr_payload(&payload)
}

/// Generate a new QR code from the decoded payload and render
/// it using Unicode half-block characters.
///
/// Dark modules become terminal background pixels, while light
/// modules become terminal foreground pixels. This produces a
/// white-on-black QR suitable for cameras and terminals.
fn render_qr_payload(payload: &str) -> Result<()> {
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::L)
        .context("failed to generate terminal QR code")?;

    let modules = code.to_colors();
    let width = code.width();
    let total_width = width + QR_QUIET_ZONE * 2;

    println!();

    // Top quiet zone.
    for _ in 0..QR_QUIET_ZONE {
        print!("  ");

        for _ in 0..total_width {
            print!("  ");
        }

        println!();
    }

    let mut row = 0usize;

    while row < width {
        print!("  ");

        // Left quiet zone.
        for _ in 0..QR_QUIET_ZONE {
            print!("  ");
        }

        for col in 0..width {
            let top = modules[row * width + col];

            let bottom = if row + 1 < width {
                modules[(row + 1) * width + col]
            } else {
                qrcode::Color::Light
            };

            match (top, bottom) {
                // Both modules are dark: terminal background is dark.
                (qrcode::Color::Dark, qrcode::Color::Dark) => {
                    print!(" ");
                }

                // Top is light, bottom is dark.
                (qrcode::Color::Light, qrcode::Color::Dark) => {
                    print!("▀");
                }

                // Top is dark, bottom is light.
                (qrcode::Color::Dark, qrcode::Color::Light) => {
                    print!("▄");
                }

                // Both modules are light.
                (qrcode::Color::Light, qrcode::Color::Light) => {
                    print!("█");
                }
            }
        }

        // Right quiet zone.
        for _ in 0..QR_QUIET_ZONE {
            print!("  ");
        }

        println!();

        row += 2;
    }

    // Bottom quiet zone.
    for _ in 0..QR_QUIET_ZONE {
        print!("  ");

        for _ in 0..total_width {
            print!("  ");
        }

        println!();
    }

    Ok(())
}

fn normalize_preview(value: &str) -> String {
    value
        .lines()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut out = String::new();

    for _ in 0..max_chars {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => return out,
        }
    }

    if chars.next().is_some() {
        if max_chars <= 1 {
            "…".to_string()
        } else {
            let mut truncated = out;
            truncated.pop();
            truncated.push('…');
            truncated
        }
    } else {
        out
    }
}

fn format_age(timestamp: i64) -> String {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return "?".to_string();
    };

    let now = now.as_secs() as i64;
    let delta = (now - timestamp).max(0);

    match delta {
        0..=59 => "now".to_string(),
        60..=3_599 => format!("{}m", delta / 60),
        3_600..=86_399 => format!("{}h", delta / 3_600),
        86_400..=604_799 => format!("{}d", delta / 86_400),
        _ => format!("{}w", delta / 604_800),
    }
}
