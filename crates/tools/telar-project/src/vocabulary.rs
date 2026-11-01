//! The words a `.rsx` writes bare for a value every component shares: `size:small`, `variant:ghost`, `tone:danger`.
//!
//! The list is closed and is the one place the words are spelled. Each axis is one type, `ControlSize`, `Variant` or `Tone`, and the `telar` facade turns the list into `telar::kw`, a name per word for the value it writes, and into each type's words, matched exhaustively both ways so a word added here is a compile error until its type has the value. The editor completes a prop named after an axis from it, and the same word is the arg's text form wherever a preview, a matrix or a link writes the value.

/// One axis of the vocabulary: the prop name every component that has it uses, and its words in the order a control lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VocabularyAxis {
    pub name: &'static str,
    pub words: &'static [&'static str],
}

/// Every axis, each word in exactly one of them, so a bare word names one value of one axis.
pub const VOCABULARY: &[VocabularyAxis] = &[
    VocabularyAxis {
        name: "size",
        words: &["small", "regular", "large"],
    },
    VocabularyAxis {
        name: "variant",
        words: &["solid", "soft", "outline", "ghost"],
    },
    VocabularyAxis {
        name: "tone",
        words: &[
            "accent", "neutral", "info", "success", "warning", "danger", "ink",
        ],
    },
];

/// The axis a prop named `name` takes its words from.
pub fn vocabulary_axis(name: &str) -> Option<&'static VocabularyAxis> {
    VOCABULARY.iter().find(|axis| axis.name == name)
}

/// The axis `word` is a word of.
pub fn vocabulary_axis_of(word: &str) -> Option<&'static VocabularyAxis> {
    VOCABULARY.iter().find(|axis| axis.words.contains(&word))
}

#[cfg(test)]
#[path = "vocabulary_test.rs"]
mod tests;
