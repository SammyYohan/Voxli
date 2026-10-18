//! Post-traitement — niveaux 0 et 1 (RF-26, RF-23, RF-24).
//!
//! - Niveau 0 (`PassThroughProcessor`) : identité — le texte du moteur est
//!   inséré tel quel. C'est le **défaut produit** en mode verbatim (RF-27).
//! - Niveau 1 (`FrenchProcessor`) : règles locales **déterministes**, sans
//!   réseau et sans modèle : dictionnaire métier du profil (RF-23), césure
//!   des hésitations, casse de début de phrase, typographie française
//!   (espaces insécables). Le niveau 2 (petit modèle local) et le niveau 3
//!   (distant, jamais sans consentement RF-36) réutiliseront ce même trait.
//!
//! Garde RF-27 : ces règles ne réécrivent **jamais les chiffres** — le test
//! T-27 vérifie la conservation du nombre.

use voxli_core::{PostError, PostProcessor, Profile, Style};

/// Niveau 0 : aucun post-traitement.
pub struct PassThroughProcessor;

impl PostProcessor for PassThroughProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError> {
        match profile.style {
            // Verbatim garanti : identité, jamais altérée.
            Style::Verbatim => Ok(text.to_string()),
            // Identité assumée : la cascade se limite pour l'instant au
            // niveau 0 ; le niveau 1 est activé par `postProcessing=rules`
            // avec `FrenchProcessor` (assemblage).
            Style::Punctuated => Ok(text.to_string()),
        }
    }
}

/// Niveau 1 : post-traitement français local — dictionnaire du profil puis
/// règles déterministes. Aucun appel réseau, aucune dépendance.
pub struct FrenchProcessor;

impl PostProcessor for FrenchProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError> {
        // RF-23 : le dictionnaire est un **opt-in explicite par profil** — il
        // s'applique aussi en verbatim (c'est son rôle : corriger un nom
        // propre mal transcrit sans toucher au reste).
        let text = if profile.dictionary.is_empty() {
            text.to_string()
        } else {
            apply_dictionary(text, &profile.dictionary)
        };
        match profile.style {
            Style::Verbatim => Ok(text),
            Style::Punctuated => Ok(apply_french_rules(&text)),
        }
    }
}

// --------------------------------------------------------- règles FR -------

/// Hésitations courantes de dictée (RF-26) — comparaison normalisée
/// (casse/accents ignorés), uniquement en mot entier.
const HESITATIONS: &[&str] = &[
    "euh", "euhh", "euhhh", "heu", "heuu", "hum", "hmm", "ben", "bah",
];

/// Applique les règles de niveau 1. Garde RF-27 : la séquence de chiffres
/// du texte ne change jamais.
pub fn apply_french_rules(text: &str) -> String {
    // 1. Espaces multiples → simple, bordures nettoyées.
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }

    // 2. Hésitations : suppression en mot entier (position indifférente).
    let mut tokens: Vec<String> = Vec::new();
    for token in collapsed.split(' ') {
        let tok_norm = normalize_lossy(token);
        let bare = tok_norm.trim_matches(|c: char| !c.is_alphanumeric());
        if HESITATIONS.iter().any(|h| *h == bare) {
            continue;
        }
        tokens.push(clean_token_edges(token));
    }
    let joined = tokens.join(" ");
    if joined.is_empty() {
        return String::new();
    }

    // 3. Cassé de début de phrase : première lettre du texte et de toute
    //    phrase suivant . ! ? (les tirets listes restent en l'état).
    let mut chars: Vec<char> = joined.chars().collect();
    capitalize_sentence_starts(&mut chars);

    // 4. Typographie française : espace insécable avant : ; ! ? % € —
    //    NE touche jamais aux chiffres proprement dits.
    let out: String = chars.into_iter().collect();
    apply_french_spacing(&out)
}

/// Nettoie les accolades/pieds de tokens (espaces résiduels, virgules
/// multipliées) sans toucher au contenu.
fn clean_token_edges(token: &str) -> String {
    token.trim_matches(',').to_string()
}

/// Insécable (\u{00A0}) avant les ponctuations doubles et unités monétaires/
/// pourcentage : une espace ordinaire est remplacée, une absence d'espace
/// est comblée sauf continuation de type URL (…://).
fn apply_french_spacing(text: &str) -> String {
    let nbsp = '\u{00A0}';
    let mut out = String::with_capacity(text.len() + 4);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, ':' | ';' | '!' | '?' | '%' | '€') {
            let trailing = out.ends_with(' ');
            // Continuation URL (…://) : ne rien insérer.
            let colon_url = c == ':' && matches!(chars.peek(), Some('/'));
            if trailing {
                out.pop();
                out.push(nbsp);
            } else if !colon_url && !out.is_empty() {
                let prev_alnum = out
                    .chars()
                    .last()
                    .map(|p| p.is_alphanumeric())
                    .unwrap_or(false);
                if prev_alnum {
                    out.push(nbsp);
                }
            }
            out.push(c);
            // Espace après la ponctuation forte (sauf fin de texte).
            if let Some(&n) = chars.peek() {
                if n.is_alphanumeric() {
                    out.push(' ');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Majuscule en tête de texte et après . ! ? — in place sur le tampon char.
fn capitalize_sentence_starts(chars: &mut [char]) {
    let mut at_start = true;
    for i in 0..chars.len() {
        let c = chars[i];
        if at_start {
            if c.is_alphabetic() {
                for up in c.to_uppercase() {
                    chars[i] = up;
                    break;
                }
                at_start = false;
            } else if !matches!(c, ' ' | '\'' | '’' | '"') {
                at_start = false;
            }
        } else if c == '.' || c == '!' || c == '?' {
            at_start = true;
        }
    }
}

// ------------------------------------------------------ dictionnaire -------

/// Format d'entrée du dictionnaire (RF-23) : `cible|variante1|variante2`.
/// Variante absente → la cible est son propre déclencheur (utile pour
/// capitaliser un sigle métier, ex. `CEFIR`).
pub fn parse_dictionary_entry(entry: &str) -> Option<(String, Vec<String>)> {
    let mut parts = entry.split('|');
    let target = parts.next()?.trim().to_string();
    if target.is_empty() {
        return None;
    }
    let variants: Vec<String> = parts
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    Some((target, variants))
}

/// Applique le dictionnaire au texte : remplacements insensibles à la casse
/// et aux accents (variantes déclarées), puis correction floue de mots
/// entiers (distance d'édition ≤ 1, mots ≥ 5 lettres) vers le terme cible.
pub fn apply_dictionary(text: &str, entries: &[String]) -> String {
    let mut out = text.to_string();
    for entry in entries {
        let Some((target, variants)) = parse_dictionary_entry(entry) else {
            continue;
        };
        for variant in std::iter::once(target.clone()).chain(variants.iter().cloned()) {
            if variant.is_empty() {
                continue;
            }
            out = replace_insensitive(&out, &variant, &target);
        }
        out = fuzzy_replace(&out, &target);
    }
    out
}

/// Remplacements insensibles casse/accents d'une occurrence exacte.
fn replace_insensitive(text: &str, pattern: &str, replacement: &str) -> String {
    let hay = normalize_lossy(text);
    let needle = normalize_lossy(pattern);
    let hchars: Vec<char> = hay.chars().collect();
    let nchars: Vec<char> = needle.chars().collect();
    if nchars.is_empty() || hchars.len() < nchars.len() {
        return text.to_string();
    }
    let orig: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0usize;
    while i < hchars.len() {
        let hit = hchars.len() - i >= nchars.len()
            && (0..nchars.len()).all(|k| hchars[i + k] == nchars[k]);
        if hit {
            // Respect de la casse locale : initiale majuscule si l'occurrence
            // d'origine commence par une majuscule.
            let capitalized = orig[i].is_uppercase();
            let rep = if capitalized {
                let mut cs = replacement.chars();
                match cs.next() {
                    Some(first) => {
                        let mut s = String::with_capacity(replacement.len());
                        s.extend(first.to_uppercase());
                        s.push_str(cs.as_str());
                        s
                    }
                    None => replacement.to_string(),
                }
            } else {
                replacement.to_string()
            };
            out.push_str(&rep);
            i += nchars.len();
        } else {
            out.push(orig[i]);
            i += 1;
        }
    }
    out
}

/// Correction floue de mots entiers (distance ≤ 1, mots ≥ 5 lettres) —
/// les termes du dictionnaire servent de référence de longueur proche.
fn fuzzy_replace(text: &str, target: &str) -> String {
    let norm_target = normalize_lossy(target);
    if norm_target.chars().count() < 5 {
        return text.to_string(); // garde anti-fausses-corrections
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut segment: Vec<char> = Vec::new(); // mot courant (original)
    let mut seg_norm: Vec<char> = Vec::new();
    let flush = |out: &mut String, segment: &mut Vec<char>, seg_norm: &mut Vec<char>| {
        if seg_norm.is_empty() {
            return;
        }
        let is_word = seg_norm.iter().all(|c| c.is_alphanumeric());
        let long_enough = seg_norm.len() >= 5;
        if is_word
            && long_enough
            && levenshtein(seg_norm, norm_target.chars().collect::<Vec<_>>().as_slice()) <= 1
        {
            let capitalized = segment[0].is_uppercase();
            let rep = if capitalized {
                let mut cs = target.chars();
                match cs.next() {
                    Some(first) => {
                        let mut s = String::with_capacity(target.len());
                        s.extend(first.to_uppercase());
                        s.push_str(cs.as_str());
                        s
                    }
                    None => target.to_string(),
                }
            } else {
                target.to_string()
            };
            out.push_str(&rep);
        } else {
            out.extend(segment.iter());
        }
        segment.clear();
        seg_norm.clear();
    };
    for &c in &chars {
        if c.is_alphanumeric() {
            segment.push(c);
            seg_norm.push(deaccent(c.to_lowercase().next().unwrap_or(c)));
        } else {
            flush(&mut out, &mut segment, &mut seg_norm);
            out.push(c);
        }
    }
    flush(&mut out, &mut segment, &mut seg_norm);
    out
}

/// Minuscule + accents français retirés (longueur conservée).
fn normalize_lossy(s: &str) -> String {
    s.chars()
        .map(|c| {
            let lower = c.to_lowercase().next().unwrap_or(c);
            deaccent(lower)
        })
        .collect()
}

fn deaccent(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ÿ' => 'y',
        'ç' => 'c',
        _ => c,
    }
}

/// Distance d'édition classique (Levenshtein) — implémentation bornée, les
/// mots traités font au plus quelques dizaines de caractères.
pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    let (m, n) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0usize; n + 1];
    for i in 1..=m {
        cur[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[n]
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxli_core::{InsertionPolicy, Style};

    fn profile(style: Style, dictionary: &[&str]) -> Profile {
        Profile {
            style,
            insertion: InsertionPolicy::ClipboardFirst,
            dictionary: dictionary.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn t_verbatim_is_identity_without_dictionary() {
        let p = PassThroughProcessor;
        let prof = profile(Style::Verbatim, &[]);
        let out = p.process("  texte   brut  ", &prof).unwrap();
        assert_eq!(out, "  texte   brut  ");
    }

    #[test]
    fn t_rules_collapse_and_capitalize() {
        let out = apply_french_rules("euh  bonjour , ceci  est un test . merci bien");
        assert!(!out.to_lowercase().contains("euh"));
        assert!(out.starts_with("Bonjour"));
        // Fin de phrase suivie de nouvelle phrase.
        let out2 = apply_french_rules("voilà la première. et la deuxième");
        assert!(
            out2.contains(". Et la deuxième"),
            "majuscule après point : {out2}"
        );
    }

    #[test]
    fn t_rules_nbsp_before_strong_punctuation() {
        let out = apply_french_rules("salut ! comment ça va : bien ;");
        assert!(out.contains('\u{00A0}'), "insécable attendue : {out:?}");
        assert!(!out.contains(" !"));
        assert!(!out.contains(" :"));
    }

    #[test]
    fn t_rules_numbers_preserved_rf27() {
        let src = "le prix est 1234,56 en 2024 puis 3.14159 %";
        let out = apply_french_rules(src);
        let digits = |s: &str| -> usize { s.chars().filter(|c| c.is_ascii_digit()).count() };
        assert_eq!(digits(src), digits(&out), "RF-27 : chiffres conservés");
        assert!(out.contains("1234,56"));
        assert!(out.contains("3.14159"));
    }

    #[test]
    fn t_dictionary_exact_and_case_preserving() {
        let out = apply_dictionary("Le dossier CEFIR avance ; merci cefir.", &["CEFIR".into()]);
        assert_eq!(out, "Le dossier CEFIR avance ; merci CEFIR.");
    }

    #[test]
    fn t_dictionary_variant_replacement() {
        let out = apply_dictionary(
            "Un appel est prévu, chaque appel aussi.",
            &["réunion|appels|appel".into()],
        );
        assert_eq!(out, "Un réunion est prévu, chaque réunion aussi.");
    }

    /// T-23 : « métalurgie » est corrigée en « métallurgie » (distance ≤ 1).
    #[test]
    fn t_dictionary_fuzzy_correction() {
        let out = apply_dictionary(" rapport de la métalurgie interne", &["métallurgie".into()]);
        assert!(out.contains("métallurgie"), "obtenu : {out}");
        // Les petits mots ne sont jamais touchés (garde ≥ 5 lettres).
        let guarded = apply_dictionary("ce son", &["raison".into()]);
        assert_eq!(guarded, "ce son");
    }

    #[test]
    fn t_french_processor_pipeline() {
        let p = FrenchProcessor;
        let prof = profile(Style::Punctuated, &["CEFIR".into()]);
        let out = p.process("euh bonjour cefir", &prof).unwrap();
        assert_eq!(out, "Bonjour CEFIR");

        let verbatim = profile(Style::Verbatim, &["CEFIR|cefir".into()]);
        let out2 = p.process("euh cefir dossier", &verbatim).unwrap();
        // Verbatim : pas de règles de ponctuation, mais dictionnaire oui.
        assert_eq!(out2, "euh CEFIR dossier");
    }
}
