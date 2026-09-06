//! Spacing and capitals, after the other stages have had their way.
//!
//! Nothing here is clever, and that is deliberate. It fixes the four things
//! that look wrong to a reader at a glance:
//!
//! - a space before `,` `.` `?` `!` `;` `:` `)` `]`
//! - a space after `(` `[` or an opening quote
//! - two spaces where one was meant
//! - a lowercase letter starting a sentence
//!
//! It runs over the whole transcript, not only over marks this app inserted.
//! That is a real choice: the decoder's own output goes through it too, so a
//! sentence Whisper started in lowercase gets a capital. On prose that is what
//! people want; in a terminal it is not, which is why the whole formatting pass
//! is switchable per app rather than this stage trying to guess.

/// Marks that must never have a space in front of them.
const CLOSES: &[char] = &[',', '.', '?', '!', ';', ':', ')', ']', '}', '\u{201d}', '%'];

/// Marks that must never have a space after them.
const OPENS: &[char] = &['(', '[', '{', '\u{201c}', '$', '@', '#'];

/// Marks that end a sentence, so the next letter is capitalised.
const ENDERS: &[char] = &['.', '?', '!'];

/// Marks that French sets off with a space in front, unlike every other
/// language here. Stripping it would be a typography error a French reader
/// notices immediately.
const FR_SPACED_MARKS: &[char] = &['?', '!', ';', ':'];

/// Fix spacing around punctuation and capitalise sentence starts.
pub fn apply(text: &str, language: Option<&str>) -> String {
    let french = language
        .map(|l| l.to_lowercase())
        .is_some_and(|l| l.split(['-', '_']).next() == Some("fr"));
    capitalise_sentences(&fix_spacing(text, french))
}

/// Remove spaces that punctuation should not have around it, and collapse
/// runs of spaces. Newlines are preserved exactly: they are structure, not
/// spacing, and the punctuation stage may have just put them there.
fn fix_spacing(text: &str, french: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());

    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            // A space before a closing mark is dropped.
            if chars[i + 1..]
                .iter()
                .find(|c| **c != ' ')
                .is_some_and(|next| {
                    CLOSES.contains(next) && !(french && FR_SPACED_MARKS.contains(next))
                })
            {
                continue;
            }
            // A space that follows an opening mark is dropped.
            if out.chars().last().is_some_and(|prev| OPENS.contains(&prev)) {
                continue;
            }
            // Collapse a run, and never leave a space hanging off a line break.
            if out.ends_with(' ') || out.ends_with('\n') || out.is_empty() {
                continue;
            }
        }
        // A space *before* a newline is trailing whitespace on that line.
        if c == '\n' {
            while out.ends_with(' ') {
                out.pop();
            }
        }
        out.push(c);
    }
    out
}

/// Capitalise the first letter of the text and of every sentence after it.
///
/// "After a sentence ender" means the next letter, skipping spaces and closing
/// quotes — so `he said "no." then` capitalises `then`. A line break counts as
/// an ender too: someone who said "new paragraph" has finished a thought, even
/// if they did not say "period" first.
fn capitalise_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // The start of the text is the start of a sentence.
    let mut pending = true;

    for c in text.chars() {
        if pending && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            pending = false;
            continue;
        }
        if ENDERS.contains(&c) || c == '\n' {
            pending = true;
        } else if c.is_alphanumeric() {
            // Any other content means we are inside a sentence again. Spaces
            // and closing quotes deliberately leave `pending` alone so they can
            // sit between the mark and the next sentence.
            pending = false;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Most of these are language-neutral; English is the representative case.
    fn en(text: &str) -> String {
        apply(text, Some("en"))
    }

    #[test]
    fn spaces_before_closing_marks_are_removed() {
        assert_eq!(en("hello , world ."), "Hello, world.");
        assert_eq!(en("really ? yes !"), "Really? Yes!");
        assert_eq!(en("an aside ) after"), "An aside) after");
    }

    #[test]
    fn spaces_after_opening_marks_are_removed() {
        // The capital is the sentence-start rule, not the bracket rule.
        assert_eq!(en("( an aside )"), "(An aside)");
        assert_eq!(en("see ( an aside )"), "See (an aside)");
        assert_eq!(en("costs $ 40"), "Costs $40");
    }

    #[test]
    fn runs_of_spaces_collapse() {
        assert_eq!(en("too   many    spaces"), "Too many spaces");
    }

    /// Newlines are structure. Collapsing them into spaces would undo the
    /// "new paragraph" the user just asked for.
    #[test]
    fn newlines_survive_and_shed_their_trailing_spaces() {
        assert_eq!(en("one \n two"), "One\nTwo");
        assert_eq!(en("one \n\n two"), "One\n\nTwo");
    }

    #[test]
    fn sentences_start_with_a_capital() {
        assert_eq!(en("first one. second one"), "First one. Second one");
        assert_eq!(en("what? no! really"), "What? No! Really");
    }

    /// A capital inside a sentence is the speaker's, not ours to change.
    #[test]
    fn existing_capitals_are_left_alone() {
        assert_eq!(en("we deployed Kubernetes today"), "We deployed Kubernetes today");
    }

    /// A decimal point is not a sentence ender in practice — but it does set
    /// `pending`, and the digit after it is not alphabetic, so nothing is
    /// wrongly capitalised. Pinned because it is the obvious thing to break.
    #[test]
    fn a_decimal_point_does_not_capitalise_anything() {
        assert_eq!(en("it costs 3.50 today"), "It costs 3.50 today");
    }

    /// French sets its high punctuation off with a space. Stripping it is a
    /// typography error a French reader notices immediately, so the rule that
    /// removes spaces before punctuation has to know the language.
    #[test]
    fn french_keeps_the_space_before_high_punctuation() {
        assert_eq!(apply("vraiment ?", Some("fr")), "Vraiment ?");
        assert_eq!(apply("alors : voici", Some("fr")), "Alors : voici");
        // A comma and a full stop take no space in French either.
        assert_eq!(apply("bonjour , monde .", Some("fr")), "Bonjour, monde.");
        // And the exception is French-only.
        assert_eq!(en("really ?"), "Really?");
    }

    #[test]
    fn empty_input_is_empty_output() {
        assert_eq!(en(""), "");
    }
}
