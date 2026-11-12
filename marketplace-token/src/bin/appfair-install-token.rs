// Copyright © The Daybrite Project
// SPDX-License-Identifier: MPL-2.0

//! Mint or check one install verification token from the command line, for testing a marketplace
//! server's configuration by hand:
//!
//! ```text
//! appfair-install-token sign --key key.pem --kid <key-id> --iss <marketplace-item-id> \
//!     --bid <bundle-id> [--iid <item-id>] [--vid <version-id>] [--redownload] [--lifetime <s>]
//! appfair-install-token verify --public public.pem <token>
//! appfair-install-token marketplace --key key.pem --iss <marketplace-apple-id> \
//!     --pid <developer-id> [--lifetime <s>]
//! ```
//!
//! `marketplace` makes the token a developer uploads to App Store Connect (Users and Access →
//! Integrations → Marketplace) to let the marketplace distribute their apps.

use app_fair_marketplace_token::{Claims, DownloadType, Signer, verify};

fn main() {
    if let Err(e) = run(std::env::args().skip(1).collect()) {
        eprintln!("appfair-install-token: {e}");
        std::process::exit(1);
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn run(args: Vec<String>) -> Result<(), String> {
    let (command, rest) = args.split_first().ok_or_else(usage)?;
    let mut flags = std::collections::HashMap::new();
    let mut positional = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.strip_prefix("--") {
            Some("redownload") => {
                flags.insert("redownload".to_string(), String::new());
            }
            Some(name) => {
                let value = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
                flags.insert(name.to_string(), value.clone());
            }
            None => positional.push(arg.clone()),
        }
    }
    let flag = |name: &str| {
        flags
            .get(name)
            .cloned()
            .ok_or_else(|| format!("--{name} is required"))
    };
    let read = |path: String| std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"));
    let id = |name: &str| -> Result<Option<u64>, String> {
        flags
            .get(name)
            .map(|v| v.parse().map_err(|_| format!("--{name} must be a number")))
            .transpose()
    };
    match command.as_str() {
        "sign" => {
            let signer =
                Signer::from_pem(&read(flag("key")?)?, flag("kid")?).map_err(|e| e.to_string())?;
            let dtype = if flags.contains_key("redownload") {
                DownloadType::Redownload
            } else {
                DownloadType::Download
            };
            let mut claims = Claims::new(flag("iss")?, flag("bid")?, dtype);
            if let Some(item) = id("iid")? {
                claims = claims.item(item);
            }
            if let Some(version) = id("vid")? {
                claims = claims.version(version);
            }
            if let Some(seconds) = flags.get("lifetime") {
                claims = claims.lifetime(
                    seconds
                        .parse()
                        .map_err(|_| "--lifetime must be a number of seconds".to_string())?,
                );
            }
            println!(
                "{}",
                signer.sign(&claims, now()).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        "marketplace" => {
            // The key id is App Store Connect's business, not this token's: Apple's example header
            // carries none.
            let signer = Signer::from_pem(&read(flag("key")?)?, "").map_err(|e| e.to_string())?;
            let lifetime = match flags.get("lifetime") {
                Some(s) => s
                    .parse()
                    .map_err(|_| "--lifetime must be a number of seconds".to_string())?,
                // Six days: as long as Apple allows, with a day's margin for clocks.
                None => 6 * 24 * 60 * 60,
            };
            println!(
                "{}",
                signer
                    .marketplace_token(&flag("iss")?, &flag("pid")?, now(), lifetime)
                    .map_err(|e| e.to_string())?
            );
            Ok(())
        }
        "verify" => {
            let token = positional.first().ok_or_else(usage)?;
            let verified =
                verify(token, &read(flag("public")?)?, now()).map_err(|e| e.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "header": verified.header,
                    "claims": verified.claims,
                }))
                .map_err(|e| e.to_string())?
            );
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: appfair-install-token sign --key <pem> --kid <id> --iss <item-id> --bid <bundle-id> \
     [--iid <item-id>] [--vid <version-id>] [--redownload] [--lifetime <seconds>]\n       \
     appfair-install-token verify --public <pem> <token>\n       \
     appfair-install-token marketplace --key <pem> --iss <marketplace-apple-id> --pid <developer-id> \
     [--lifetime <seconds>]"
        .to_string()
}
