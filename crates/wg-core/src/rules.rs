use globset::Glob;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(rename = "match")]
    pub pattern: String,
    pub label: String,
    pub hint: String,
}

#[derive(Debug, Deserialize)]
struct RuleFile { rule: Vec<Rule> }

pub fn load_rules() -> Vec<Rule> {
    let mut result = toml::from_str::<RuleFile>(include_str!("../rules/default.toml"))
        .map_or_else(|_| Vec::new(), |r| r.rule);
    let custom = dirs::config_dir().unwrap_or_else(|| ".".into()).join("whatgrew/rules.toml");
    if let Ok(source) = std::fs::read_to_string(custom) {
        if let Ok(extra) = toml::from_str::<RuleFile>(&source) { result.extend(extra.rule); }
    }
    result
}

pub fn label_for(path: &str, rules: &[Rule]) -> Option<(String, String)> {
    let normalized = path.replace('\\', "/");
    rules.iter().find_map(|rule| {
        let glob = Glob::new(&rule.pattern).ok()?;
        glob.compile_matcher().is_match(Path::new(&normalized))
            .then(|| (rule.label.clone(), rule.hint.clone()))
    })
}
