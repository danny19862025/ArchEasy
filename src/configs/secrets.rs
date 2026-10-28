use super::workflow::Plan;

// Report only locations and categories; never echo a credential into the UI/log.
pub(super) fn check(plan: &Plan) -> Result<(), String> {
    let mut findings = Vec::new();
    for (name, bytes) in &plan.files {
        let basename = std::path::Path::new(name)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if [".env", ".netrc", ".npmrc", "id_rsa", "id_ed25519"].contains(&basename.as_ref()) {
            findings.push(format!("{name}: credential file"));
        }
        for (index, line) in String::from_utf8_lossy(bytes).lines().enumerate() {
            let known_token = [
                "ghp_",
                "github_pat_",
                "gho_",
                "ghu_",
                "ghs_",
                "ghr_",
                "xoxb-",
                "xoxp-",
                "xoxa-",
                "sk-",
                "AKIA",
                "ASIA",
            ]
            .iter()
            .any(|prefix| {
                line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                    .any(|word| word.starts_with(prefix) && word.len() >= prefix.len() + 16)
            });
            let private_key = line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----");
            let assignment = line.trim().trim_start_matches("export ");
            let credential = assignment
                .split_once(['=', ':'])
                .is_some_and(|(key, value)| {
                    let key = key
                        .trim()
                        .trim_start_matches('{')
                        .trim()
                        .trim_matches(['\'', '"'])
                        .to_ascii_lowercase();
                    let value = value.trim().trim_end_matches(',').trim_matches(['\'', '"']);
                    let secret_key = [
                        "password",
                        "passwd",
                        "token",
                        "api_key",
                        "apikey",
                        "secret",
                        "private_key",
                    ]
                    .iter()
                    .any(|part| key == *part || key.ends_with(&format!("_{part}")));
                    secret_key
                        && !value.is_empty()
                        && !value.starts_with(['$', '<'])
                        && ![
                            "changeme",
                            "your_token_here",
                            "your_api_key_here",
                            "example",
                            "null",
                            "none",
                        ]
                        .contains(&value.to_ascii_lowercase().as_str())
                });
            if known_token || private_key || credential {
                findings.push(format!("{name}:{}: possible secret", index + 1));
            }
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Publishing blocked: possible secrets found. Remove credentials from the preset and review again.\n{}",
            findings.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_credentials_without_disclosing_values() {
        for content in [
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "token = ghp_0123456789abcdefghijk",
            "export API_KEY='real-credential'",
            "{\"password\": \"secret-value\"}",
            "AWS_ACCESS_KEY_ID=AKIA0123456789ABCDEF",
        ] {
            let plan = Plan {
                id: "fixture".into(),
                name: "Fixture".into(),
                dependencies: vec![],
                managed_parents: vec![],
                files: vec![("app/settings".into(), content.as_bytes().to_vec())],
            };
            let error = check(&plan).unwrap_err();
            assert!(error.contains("app/settings:1"));
            assert!(!error.contains(content));
        }
        let plan = Plan {
            id: "fixture".into(),
            name: "Fixture".into(),
            dependencies: vec![],
            managed_parents: vec![],
            files: vec![(
                "app/settings".into(),
                b"theme=dark\napi_key=$API_KEY\npassword=\"\"".to_vec(),
            )],
        };
        assert!(check(&plan).is_ok());
    }
}
