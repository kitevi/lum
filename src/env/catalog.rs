pub(crate) const ALIASES: &[(&str, &str)] = &[
    ("baro", "BARO_DB_URL"),
    ("deepseek", "DEEPSEEK_API_KEY"),
    ("exa", "EXA_API_KEY"),
    ("hypercharm", "HYPERCHARM_API_KEY"),
    ("neuralwatt", "NEURALWATT_API_KEY"),
    ("opencode", "OPENCODE_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("synthetic", "SYNTHETIC_API_KEY"),
    ("terr", "TERR_DB_URL"),
    ("zro", "ZRO_API_KEY"),
];

pub(crate) const FORCED_ENV: &[(&str, &str)] = &[("npm_config_ignore_scripts", "true")];

pub(crate) fn variable_for_alias(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find_map(|(key, variable)| (*key == alias).then_some(*variable))
}

pub(crate) fn mask_secret(value: &str) -> String {
    if value.len() <= 8 {
        "********".to_owned()
    } else {
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}
