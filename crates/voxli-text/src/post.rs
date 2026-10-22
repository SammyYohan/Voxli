//! Post-processing — levels 0 and 1 (RF-26, RF-23, RF-24).
//!
//! - Level 0 (`PassThroughProcessor`): identity — the engine's text is
//!   inserted as-is. This is the **production default** in verbatim mode
//!   (RF-27).
//! - Level 1 (`FrenchProcessor`): **deterministic** local rules, no
//!   network and no model: profile business dictionary (RF-23), hesitation
//!   trimming, sentence-start capitalization, French typography
//!   (non-breaking spaces). Level 2 (small local model) and level 3
//!   (remote, never without consent RF-36) will reuse this same trait.
//!
//! RF-27 guard: these rules **never rewrite numbers** — test T-27 checks
//! that the number is preserved.

use voxli_core::{PostError, PostProcessor, Profile, Style};

/// Level 0: no post-processing.
pub struct PassThroughProcessor;

impl PostProcessor for PassThroughProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError> {
        match profile.style {
            // Guaranteed verbatim: identity, never altered.
            Style::Verbatim => Ok(text.to_string()),
            // Deliberate identity: for now the cascade stops at level 0;
            // level 1 is enabled by `postProcessing=rules` with the
            // `FrenchProcessor` (wiring).
            Style::Punctuated => Ok(text.to_string()),
        }
    }
}

/// Level 1: local French post-processing — the profile's dictionary first,
/// then deterministic rules. No network calls, no dependencies.
pub struct FrenchProcessor;

impl PostProcessor for FrenchProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError> {
        // RF-23: the dictionary is an **explicit per-profile opt-in** — it
        // also applies in verbatim mode (that is its role: fix a badly
        // transcribed proper noun without touching anything else).
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

// ----------------------------------------------------- FR rules -----------

/// Common spoken-dictation hesitations (RF-26) — normalized comparison
/// (case/accents ignored), whole word only.
const HESITATIONS: &[&str] = &[
    "euh", "euhh", "euhhh", "euhhhh", "heu", "heuu", "heuuu", "hum", "hmm", "hmmm", "mmh", "mmm",
    "ben", "bah", "euhm", "uh", "um",
];

/// Words whose consecutive repetition is legitimate in French and must
/// never be removed (reflexive pronouns, intensifying adverbs, affirmations).
const REPETITION_EXCLUSIONS: &[&str] = &[
    // Reflexive pronouns ("nous nous levons", "vous vous souvenez")
    "nous", "vous", // Intensifying adverbs and adjectives repeated on purpose
    "tres", "trop", "bien", "vite", "beaucoup", "presque", "plus", "moins", "tout", "jamais",
    // Repeated spoken affirmations / negations ("oui oui", "non non")
    "oui", "non", "si",
];

/// Applies the level-1 rules (Magic Formatting v1).
/// RF-27 guard: the text's digit sequence never changes.
pub fn apply_french_rules(text: &str) -> String {
    // 1. Multiple spaces → single, trimmed boundaries.
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return String::new();
    }

    // 2. Tokenization with spoken hesitation removal and natural
    //    reattachment of isolated punctuation marks.
    let raw_tokens: Vec<&str> = collapsed.split(' ').collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut dropped_hesitation = false;

    for &raw in &raw_tokens {
        let norm = normalize_lossy(raw);
        let bare = norm.trim_matches(|c: char| !c.is_alphanumeric());

        // Identified hesitation
        if HESITATIONS.contains(&bare) {
            dropped_hesitation = true;
            // If the hesitation carried a strong terminal punctuation mark
            // (. ! ? ;), carry it over to the previous token, if any.
            let strong_punct: String = raw
                .chars()
                .filter(|c| matches!(c, '.' | '!' | '?' | ';' | ':'))
                .collect();
            if !strong_punct.is_empty() {
                if let Some(prev) = tokens.last_mut() {
                    if !prev.ends_with(|c: char| !c.is_alphanumeric()) {
                        prev.push_str(&strong_punct);
                    }
                }
            }
            continue;
        }

        // Isolated punctuation mark (e.g. "word , word" or "sentence . sentence")
        if raw == "," || raw == "." || raw == ";" || raw == ":" || raw == "!" || raw == "?" {
            if let Some(prev) = tokens.last_mut() {
                if !prev.ends_with(|c: char| !c.is_alphanumeric()) {
                    prev.push_str(raw);
                }
            }
            continue;
        }

        // If a hesitation ended with a comma before a subordinating
        // conjunction (e.g. "je pense, [euh,] qu'il faut"), clean up
        // the superfluous comma.
        if dropped_hesitation {
            dropped_hesitation = false;
            let bare_raw = norm.trim_matches(|c: char| !c.is_alphanumeric());
            if bare_raw == "que"
                || bare_raw.starts_with("qu'")
                || bare_raw.starts_with("qu’")
                || matches!(bare_raw, "qui" | "dont" | "ou" | "et")
            {
                if let Some(prev) = tokens.last_mut() {
                    if prev.ends_with(',') {
                        prev.pop();
                    }
                }
            }
        }

        tokens.push(raw.to_string());
    }

    if tokens.is_empty() {
        return String::new();
    }

    // 3. Removal of involuntary spoken repetitions (stutters):
    //    passes over 3-word blocks, then 2 words, then 1 word.
    let tokens = collapse_phrase_repetitions(&tokens, 3);
    let tokens = collapse_phrase_repetitions(&tokens, 2);
    let tokens = collapse_single_word_repetitions(&tokens);

    let joined = tokens.join(" ");
    let cleaned = clean_punctuation_edges(&joined);
    if cleaned.is_empty() {
        return String::new();
    }

    // 4. Sentence-start capitalization: first letter of the text and of
    //    any sentence following . ! ? (list dashes are left untouched).
    let mut chars: Vec<char> = cleaned.chars().collect();
    capitalize_sentence_starts(&mut chars);

    // 5. French typography: non-breaking space before : ; ! ? % € —
    //    NEVER touches actual digits.
    let out: String = chars.into_iter().collect();
    apply_french_spacing(&out)
}

/// Removes repetitions of `n`-word phrases (e.g. "dans le dans le" -> "dans le").
fn collapse_phrase_repetitions(tokens: &[String], n: usize) -> Vec<String> {
    if tokens.len() < 2 * n {
        return tokens.to_vec();
    }
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if i + 2 * n <= tokens.len() {
            let matches = (0..n).all(|k| {
                let w_a = normalize_lossy(&tokens[i + k]);
                let bare_a = w_a.trim_matches(|c: char| !c.is_alphanumeric());
                let w_b = normalize_lossy(&tokens[i + n + k]);
                let bare_b = w_b.trim_matches(|c: char| !c.is_alphanumeric());
                !bare_a.is_empty() && bare_a == bare_b && !REPETITION_EXCLUSIONS.contains(&bare_a)
            });
            if matches {
                for k in 0..n {
                    out.push(tokens[i + k].clone());
                }
                // If the second block had trailing punctuation the first block
                // lacked, recover it
                let last_punct: String = tokens[i + 2 * n - 1]
                    .chars()
                    .filter(|c| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
                    .collect();
                if !last_punct.is_empty() {
                    if let Some(last_elem) = out.last_mut() {
                        if !last_elem.ends_with(|c: char| !c.is_alphanumeric()) {
                            last_elem.push_str(&last_punct);
                        }
                    }
                }
                i += 2 * n;
                continue;
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

/// Removes single-word stutters (e.g. "je je" -> "je", "dans dans" -> "dans").
fn collapse_single_word_repetitions(tokens: &[String]) -> Vec<String> {
    if tokens.len() < 2 {
        return tokens.to_vec();
    }
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if i + 1 < tokens.len() {
            let w1_norm = normalize_lossy(&tokens[i]);
            let w1_bare = w1_norm.trim_matches(|c: char| !c.is_alphanumeric());

            let w2_norm = normalize_lossy(&tokens[i + 1]);
            let w2_bare = w2_norm.trim_matches(|c: char| !c.is_alphanumeric());

            if !w1_bare.is_empty()
                && w1_bare == w2_bare
                && !REPETITION_EXCLUSIONS.contains(&w1_bare)
            {
                // Consecutive repetition detected: absorb all following identical
                // consecutive repetitions
                let is_capitalized = tokens[i]
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false);

                let mut j = i + 1;
                let mut last_punct = String::new();
                while j < tokens.len() {
                    let w_next_norm = normalize_lossy(&tokens[j]);
                    let w_next_bare = w_next_norm.trim_matches(|c: char| !c.is_alphanumeric());
                    if w_next_bare == w1_bare {
                        let punct: String = tokens[j]
                            .chars()
                            .filter(|c| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
                            .collect();
                        if !punct.is_empty() {
                            last_punct = punct;
                        }
                        j += 1;
                    } else {
                        break;
                    }
                }

                let mut merged = tokens[i]
                    .trim_end_matches(|c: char| !c.is_alphanumeric())
                    .to_string();
                if is_capitalized {
                    let mut chars = merged.chars();
                    if let Some(first) = chars.next() {
                        merged = first.to_uppercase().collect::<String>() + chars.as_str();
                    }
                }
                if !last_punct.is_empty() {
                    merged.push_str(&last_punct);
                } else {
                    let first_punct: String = tokens[i]
                        .chars()
                        .filter(|c| matches!(c, '.' | '!' | '?' | ',' | ';' | ':'))
                        .collect();
                    merged.push_str(&first_punct);
                }

                out.push(merged);
                i = j;
                continue;
            }
        }
        out.push(tokens[i].clone());
        i += 1;
    }
    out
}

/// Cleans orphan punctuation at the boundaries and doubled commas.
fn clean_punctuation_edges(text: &str) -> String {
    // Trims commas or periods at the very start of the text
    let trimmed = text.trim_start_matches([',', ' ']);
    // Cleans doubled commas ",," or ", ,"
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_comma = false;
    for c in trimmed.chars() {
        if c == ',' {
            if prev_comma {
                continue;
            }
            prev_comma = true;
            out.push(c);
        } else {
            if c != ' ' {
                prev_comma = false;
            }
            out.push(c);
        }
    }
    out
}

/// Non-breaking space (\u{00A0}) before double punctuation and currency/
/// percentage units: an ordinary space is replaced, a missing space is
/// filled in — except for URL-like continuations (…://) and "compact
/// colons" followed by an alphanumeric (times 12:00, ratios 3:2).
fn apply_french_spacing(text: &str) -> String {
    let nbsp = '\u{00A0}';
    let mut out = String::with_capacity(text.len() + 4);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        // Compact case 12:00 / 3:2 / hour:minute: colons immediately
        // followed by an alphanumeric character are NOT French double
        // punctuation (time, ratio, code). Transcribe verbatim —
        // RF-27: dictated content stays faithful.
        if c == ':' && matches!(chars.peek(), Some(n) if n.is_alphanumeric()) {
            out.push(c);
            continue;
        }
        if matches!(c, ':' | ';' | '!' | '?' | '%' | '€') {
            let trailing = out.ends_with(' ');
            // URL continuation (…://): insert nothing.
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
            // Space after strong punctuation (except end of text).
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

/// Capitalizes the start of the text and after . ! ? — in place, on the
/// char buffer.
fn capitalize_sentence_starts(chars: &mut [char]) {
    let mut at_start = true;
    for c in chars.iter_mut() {
        if at_start {
            if c.is_alphabetic() {
                if let Some(up) = c.to_uppercase().next() {
                    *c = up;
                }
                at_start = false;
            } else if !matches!(*c, ' ' | '\'' | '’' | '"') {
                at_start = false;
            }
        } else if *c == '.' || *c == '!' || *c == '?' {
            at_start = true;
        }
    }
}

// ---------------------------------------------------- dictionary -----------

/// Dictionary entry format (RF-23): `target|variant1|variant2`.
/// No variant → the target triggers itself (useful to capitalize a
/// business acronym, e.g. `CEFIR`).
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

/// Applies the dictionary to the text: case- and accent-insensitive
/// replacements of the declared variants, then fuzzy correction of whole
/// words (edit distance ≤ 1, words ≥ 5 letters) toward the target term.
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

/// Case/accent-insensitive replacement of an exact occurrence.
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
            // Respect local casing: capital initial if the original
            // occurrence begins with a capital letter.
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

/// Fuzzy correction of whole words (distance ≤ 1, words ≥ 5 letters) —
/// dictionary terms serve as the length reference.
fn fuzzy_replace(text: &str, target: &str) -> String {
    let norm_target = normalize_lossy(target);
    if norm_target.chars().count() < 5 {
        return text.to_string(); // guard against false corrections
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut segment: Vec<char> = Vec::new(); // current word (original)
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

/// Lowercase with French accents removed (length preserved).
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

/// Classic edit distance (Levenshtein) — bounded implementation, the words
/// processed are at most a few dozen characters long.
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
        // End of sentence followed by a new sentence.
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

    /// T-23: "métalurgie" is corrected to "métallurgie" (distance ≤ 1).
    #[test]
    fn t_dictionary_fuzzy_correction() {
        let out = apply_dictionary(" rapport de la métalurgie interne", &["métallurgie".into()]);
        assert!(out.contains("métallurgie"), "obtenu : {out}");
        // Short words are never touched (≥ 5-letter guard).
        let guarded = apply_dictionary("ce son", &["raison".into()]);
        assert_eq!(guarded, "ce son");
    }

    #[test]
    fn t_french_processor_pipeline() {
        let p = FrenchProcessor;
        let prof = profile(Style::Punctuated, &["CEFIR"]);
        let out = p.process("euh bonjour cefir", &prof).unwrap();
        assert_eq!(out, "Bonjour CEFIR");

        let verbatim = profile(Style::Verbatim, &["CEFIR|cefir"]);
        let out2 = p.process("euh cefir dossier", &verbatim).unwrap();
        // Verbatim: no punctuation rules, but the dictionary applies.
        assert_eq!(out2, "euh CEFIR dossier");
    }

    #[test]
    fn t_rules_hesitations_elimination() {
        assert_eq!(apply_french_rules("euh bonjour"), "Bonjour");
        assert_eq!(
            apply_french_rules("je pense, euh, qu'il a raison"),
            "Je pense qu'il a raison"
        );
        assert_eq!(apply_french_rules("heu hum c'est prêt"), "C'est prêt");
        assert_eq!(
            apply_french_rules("c'est terminé ben voilà"),
            "C'est terminé voilà"
        );
    }

    #[test]
    fn t_rules_stutters_and_repetitions_collapse() {
        assert_eq!(
            apply_french_rules("je je pense que c'est c'est bien"),
            "Je pense que c'est bien"
        );
        assert_eq!(
            apply_french_rules("dans dans le dossier"),
            "Dans le dossier"
        );
        assert_eq!(
            apply_french_rules("dans le dans le projet"),
            "Dans le projet"
        );
        assert_eq!(
            apply_french_rules("il y a il y a une solution"),
            "Il y a une solution"
        );
        assert_eq!(
            apply_french_rules("je je je veux continuer"),
            "Je veux continuer"
        );
    }

    #[test]
    fn t_rules_legitimate_repetitions_preserved() {
        // Reflexive pronouns
        assert_eq!(
            apply_french_rules("nous nous sommes vus hier"),
            "Nous nous sommes vus hier"
        );
        assert_eq!(apply_french_rules("vous vous trompez"), "Vous vous trompez");
        // Intensifying adverbs
        assert_eq!(
            apply_french_rules("c'est très très bien"),
            "C'est très très bien"
        );
        assert_eq!(
            apply_french_rules("c'est trop trop fort"),
            "C'est trop trop fort"
        );
        // Deliberate spoken affirmation
        assert_eq!(apply_french_rules("oui oui c'est bon"), "Oui oui c'est bon");
    }
}
