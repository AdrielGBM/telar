use tower_lsp::lsp_types::SemanticToken;

pub fn decode_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32, u32)> {
    let mut result = Vec::with_capacity(data.len() / 5);
    let mut abs_line = 0u32;
    let mut abs_char = 0u32;
    for chunk in data.chunks_exact(5) {
        let delta_line = chunk[0];
        let delta_char = chunk[1];
        abs_line += delta_line;
        if delta_line != 0 {
            abs_char = delta_char;
        } else {
            abs_char += delta_char;
        }
        result.push((abs_line, abs_char, chunk[2], chunk[3], chunk[4]));
    }
    result
}

pub fn encode_tokens(tokens: &[(u32, u32, u32, u32, u32)]) -> Vec<SemanticToken> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;
    for &(line, ch, length, token_type, token_mods) in tokens {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 { ch - prev_char } else { ch };
        result.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: token_mods,
        });
        prev_line = line;
        prev_char = ch;
    }
    result
}
