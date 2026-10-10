pub(crate) const ALIASES: &[(&str, &str)] = &[
    ("baro", "BARO_DB_URL"),
    ("deepseek", "DEEPSEEK_API_KEY"),
    ("exa", "EXA_API_KEY"),
    ("hypercharm", "HYPERCHARM_API_KEY"),
    ("opencode", "OPENCODE_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("terr", "TERR_DB_URL"),
    ("tinyfish", "TINYFISH_API_KEY"),
];

pub(crate) const FORCED_ENV: &[(&str, &str)] = &[("npm_config_ignore_scripts", "true")];

pub(crate) fn variable_for_alias(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find_map(|(key, variable)| (*key == alias).then_some(*variable))
}

/// Mask by character to avoid splitting UTF-8 sequences.
pub(crate) fn mask_secret(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        "********".to_owned()
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{prefix}...{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_hides_short_values_entirely() {
        assert_eq!(mask_secret("abc"), "********");
        assert_eq!(mask_secret("12345678"), "********");
        assert_eq!(mask_secret("日本語語語"), "********");
    }

    #[test]
    fn mask_secret_keeps_first_and_last_four_characters() {
        assert_eq!(mask_secret("123456789"), "1234...6789");
        assert_eq!(mask_secret("sk-or-v1-secret"), "sk-o...cret");
    }

    #[test]
    fn mask_secret_slices_on_character_boundaries() {
        assert_eq!(mask_secret("日本語abcdefgh"), "日本語a...efgh");
        assert_eq!(mask_secret("ααααββββββ"), "αααα...ββββ");
        assert_eq!(mask_secret("😀😀😀😀😀😀😀😀😀"), "😀😀😀😀...😀😀😀😀");
    }
}
