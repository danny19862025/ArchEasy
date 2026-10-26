use serde_json::Value;
pub fn terms(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let entries: Vec<Value> =
        serde_json::from_str(include_str!("../data/search-keywords.json")).unwrap_or_default();
    let mut found = vec![];
    for entry in entries {
        if entry["phrases"].as_array().is_some_and(|phrases| {
            phrases
                .iter()
                .filter_map(Value::as_str)
                .any(|p| lower.contains(p))
        }) {
            for package in entry["packages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !found.iter().any(|p| p == package) {
                    found.push(package.to_owned());
                }
            }
        }
    }
    if found.is_empty() {
        found.push(query.to_owned());
    }
    found
}
#[cfg(test)]
mod tests {
    #[test]
    fn task_queries_and_exact_names() {
        assert_eq!(
            super::terms("video editor like premiere"),
            ["kdenlive", "shotcut", "davinci-resolve"]
        );
        assert_eq!(super::terms("neovim"), ["neovim"]);
    }
}
