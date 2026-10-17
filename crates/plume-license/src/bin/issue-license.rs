//! Outil d'émission de licences Plume (RF-38) — binaire développement.
//!
//! La paire de clés vit hors dépôt (`.keys/license-keypair.json`, ignoré
//! par git). Usages :
//!
//! ```text
//! cargo run -p plume-license --bin issue-license -- --public-key
//! cargo run -p plume-license --bin issue-license -- \
//!     --to "Sammy" --features history-on-disk,extra-profiles \
//!     --expires 2030-12-31 --out .keys/exemple.license
//! ```
//!
//! En production, la paire n'est JAMAIS sur la machine de build : ce binaire
//! est un outil de développement, pas un composant embarqué.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;

use plume_license::{sign, LicenseDoc, BLOB_FORMAT, PRODUCT, PURCHASABLE_FEATURES};

const KEYPAIR_VERSION: &str = "plume-keypair-v1";

#[derive(serde::Serialize, serde::Deserialize)]
struct KeypairFile {
    format: String,
    #[serde(rename = "seedHex")]
    seed_hex: String,
    #[serde(rename = "publicHex")]
    public_hex: String,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut keypair_path = PathBuf::from(".keys/license-keypair.json");
    let mut licensee = String::new();
    let mut product = PRODUCT.to_string();
    let mut features: Vec<String> = Vec::new();
    let mut expires = String::new();
    let mut seats = String::new();
    let mut out_path: Option<PathBuf> = None;
    let mut want_public = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--keypair" => keypair_path = PathBuf::from(it.next().expect("chemin après --keypair")),
            "--public-key" => want_public = true,
            "--to" => licensee = it.next().expect("valeur après --to").clone(),
            "--product" => product = it.next().expect("valeur après --product").clone(),
            "--features" => {
                features = it
                    .next()
                    .expect("valeur après --features")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            "--expires" => expires = it.next().expect("valeur après --expires").clone(),
            "--seats" => seats = it.next().expect("valeur après --seats").clone(),
            "--out" => out_path = Some(PathBuf::from(it.next().expect("chemin après --out"))),
            other => {
                eprintln!("argument inconnu : {other}");
                print_help();
                std::process::exit(2);
            }
        }
    }

    if want_public {
        let pair = load_or_create(&keypair_path);
        println!("{}", pair.public_hex);
        return;
    }

    if licensee.is_empty() {
        print_help();
        std::process::exit(2);
    }

    // Cohérence RF-38 : aucune fonctionnalité inconnue.
    for f in &features {
        if !PURCHASABLE_FEATURES.contains(&f.as_str()) && f != "all" {
            eprintln!(
                "fonctionnalité inconnue : {f}\nvalables : {}",
                PURCHASABLE_FEATURES.join(", ")
            );
            std::process::exit(2);
        }
    }
    if features.iter().any(|f| f == "all") {
        features = PURCHASABLE_FEATURES.iter().map(|s| s.to_string()).collect();
    }

    let pair = load_or_create(&keypair_path);
    let seed_vec = hex_to_bytes(&pair.seed_hex).expect("seed hex invalide");
    let seed: [u8; 32] = seed_vec.try_into().expect("seed de 32 octets");
    let key = SigningKey::from_bytes(&seed);

    let issued_at = rfc3339_now();
    let expires_at = if expires.is_empty() {
        None
    } else {
        Some(format!("{expires}T23:59:59Z"))
    };
    let seats_opt = seats.parse::<u32>().ok().filter(|_| !seats.is_empty());

    let doc = LicenseDoc {
        product: product.clone(),
        licensee: licensee.clone(),
        features: features.clone(),
        issued_at: issued_at.clone(),
        expires_at: expires_at.clone(),
        seats: seats_opt,
    };
    let blob = sign(&doc, &key);

    match out_path {
        Some(p) => {
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&p, &blob).expect("écriture du fichier de licence");
            println!("licence écrite : {}", p.display());
            println!("titulaire    : {licensee}");
            println!("fonctions    : {}", features.join(", "));
            println!(
                "expire       : {}",
                expires_at.as_deref().unwrap_or("jamais")
            );
        }
        None => println!("{blob}"),
    }
}

fn load_or_create(path: &PathBuf) -> KeypairFile {
    if let Ok(raw) = std::fs::read_to_string(path) {
        match serde_json::from_str(&raw) {
            Ok(p) => return p,
            Err(e) => {
                eprintln!(
                    "fichier de paires illisible ({e}) — refuse d'écraser : {}",
                    path.display()
                );
                std::process::exit(1);
            }
        }
    }
    // Génération : 32 octets de qualité cryptographique (OS RNG).
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("entropie système indisponible");
    let key = SigningKey::from_bytes(&seed);
    let pair = KeypairFile {
        format: KEYPAIR_VERSION.into(),
        seed_hex: bytes_to_hex(&seed),
        public_hex: bytes_to_hex(&key.verifying_key().to_bytes()),
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, serde_json::to_string_pretty(&pair).unwrap())
        .expect("écriture de la paire de clés");
    eprintln!(
        "paire de clés générée dans {} — DÉPLOYEZ-LA hors dépôt (git l'ignore).",
        path.display()
    );
    pair
}

fn rfc3339_now() -> String {
    // Horodatage UTC à la seconde, sans dépendance de fuseau : systime époq.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_in_months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = secs / 86_400;
    let time_rem = secs % 86_400;
    let (h, m, s) = (time_rem / 3600, (time_rem % 3600) / 60, time_rem % 60);
    let mut year = 1970u64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let d = if leap { 366 } else { 365 };
        if days < d {
            break;
        }
        days -= d;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let mut month = 0usize;
    let mut day = days;
    while month < 12 {
        let mut dm = days_in_months[month];
        if leap && month == 1 {
            dm += 1;
        }
        if day < dm {
            break;
        }
        day -= dm;
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02}T{h:02}:{m:02}:{s:02}Z",
        month + 1,
        day + 1
    )
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let b = s.trim();
    if b.len() % 2 != 0 {
        return None;
    }
    (0..b.len() / 2)
        .map(|i| u8::from_str_radix(&b[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

fn print_help() {
    eprintln!(
        "usage : issue-license [--keypair <fichier>] [--public-key]\n\
         |                     --to <titulaire> [--product <id>] [--features f1,f2|all]\n\
         |                     [--expires AAAA-MM-JJ] [--seats N] [--out <fichier>]\n\
         | (le format de blob est : {BLOB_FORMAT})"
    );
}
