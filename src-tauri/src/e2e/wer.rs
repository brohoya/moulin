//! The word error rate: the number of word changes from the reference to the transcript, for each word of the
//! reference.
use super::round;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordErrorRate {
    pub wer: f64,
    pub edits: usize,
    pub reference_words: usize,
    pub transcript_words: usize,
}

/// Words for the word error rate: lower case, no punctuation. "j'ai" gives "j" and "ai". "2.4" and "4,5" stay one word.
fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphanumeric() {
            i += 1;
            continue;
        }
        let mut word = String::new();
        loop {
            while i < chars.len() && chars[i].is_alphanumeric() {
                word.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() && matches!(chars[i], '.' | ',' | '-') && chars[i + 1].is_alphanumeric() {
                word.push(chars[i]);
                i += 1;
            } else {
                break;
            }
        }
        out.push(word);
    }
    out
}

/// The Levenshtein distance between the word lists, for each word of the reference.
pub fn word_error_rate(reference: &str, transcript: &str) -> WordErrorRate {
    let (refs, hyp) = (words(reference), words(transcript));
    let mut row: Vec<usize> = (0..=hyp.len()).collect();
    for i in 1..=refs.len() {
        let mut cur = vec![i; hyp.len() + 1];
        for j in 1..=hyp.len() {
            cur[j] = (row[j] + 1).min(cur[j - 1] + 1).min(row[j - 1] + usize::from(refs[i - 1] != hyp[j - 1]));
        }
        row = cur;
    }
    let edits = row[hyp.len()];
    WordErrorRate {
        wer: round(edits as f64 / refs.len().max(1) as f64, 4),
        edits,
        reference_words: refs.len(),
        transcript_words: hyp.len(),
    }
}
