use super::{Package, terminal};
use reqwest::{StatusCode, blocking::Client, header};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const API: &str = "https://api.github.com";
const BUILD: &str = include_str!("../scripts/github-build.sh");
type Cache = HashMap<String, (Instant, Vec<Package>)>;

pub fn valid_repo(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && !part.starts_with('-')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

fn client(token: Option<&str>) -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        header::HeaderValue::from_static("2026-03-10"),
    );
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        let mut value = header::HeaderValue::from_str(&format!("Bearer {}", token.trim()))
            .map_err(|_| "Invalid GitHub token file".to_owned())?;
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
    }
    Client::builder()
        .user_agent(concat!("ArchEasy/", env!("CARGO_PKG_VERSION")))
        .default_headers(headers)
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not initialize GitHub connection".into())
}

fn json(response: reqwest::blocking::Response) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS =>
                "GitHub request limited or denied. Wait before retrying, or add a token at ~/.config/archeasy/github_token.".into(),
            StatusCode::UNAUTHORIZED => "GitHub token was rejected. Check ~/.config/archeasy/github_token.".into(),
            _ => format!("GitHub returned HTTP {status}"),
        });
    }
    response
        .json()
        .map_err(|_| "Unexpected GitHub response".into())
}

pub fn search_github(query: &str) -> Result<Vec<Package>, String> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    // Serialize GitHub requests so rapid searches cannot exhaust the API concurrently.
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "GitHub search unavailable".to_owned())?;
    if let Some((at, packages)) = cache.get(query)
        && at.elapsed() < Duration::from_secs(300)
    {
        return Ok(packages.clone());
    }
    let token =
        match std::fs::read_to_string(gtk::glib::user_config_dir().join("archeasy/github_token")) {
            Ok(token) => Some(token),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("Could not read ~/.config/archeasy/github_token".into()),
        };
    let packages = search_with(&client(token.as_deref())?, API, query)?;
    cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    if cache.len() >= 20 {
        cache.clear();
    }
    cache.insert(query.into(), (Instant::now(), packages.clone()));
    Ok(packages)
}

fn search_with(client: &Client, api: &str, query: &str) -> Result<Vec<Package>, String> {
    let query = format!("{} PKGBUILD in:name,description,readme", query.trim());
    let data = json(
        client
            .get(format!("{api}/search/repositories"))
            .query(&[("q", query.as_str()), ("per_page", "20")])
            .send()
            .map_err(|_| "Could not reach GitHub. Check your connection.".to_owned())?,
    )?;
    let items = data["items"]
        .as_array()
        .ok_or("Unexpected GitHub repository response")?;
    // Repository search matches metadata, not filenames. Verify the build file
    // before offering a build action; four workers bound request concurrency.
    let mut results = Vec::new();
    for batch in items.iter().take(20).collect::<Vec<_>>().chunks(4) {
        let checked = std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .iter()
                .map(|item| {
                    scope.spawn(move || {
                        let Some(name) = item["full_name"].as_str().filter(|s| valid_repo(s))
                        else {
                            return Ok(None);
                        };
                        if item["archived"].as_bool() == Some(true)
                            || item["private"].as_bool() == Some(true)
                        {
                            return Ok(None);
                        }
                        let response = client
                            .get(format!("{api}/repos/{name}/contents/PKGBUILD"))
                            .send()
                            .map_err(|_| "Could not verify GitHub PKGBUILD".to_owned())?;
                        if response.status() == StatusCode::NOT_FOUND {
                            return Ok(None);
                        }
                        let file = json(response)?;
                        if file["type"] != "file" || file["size"].as_u64() == Some(0) {
                            return Ok(None);
                        }
                        Ok(Some(Package {
                            name: name.into(),
                            version: format!(
                                "★ {}",
                                item["stargazers_count"].as_u64().unwrap_or(0)
                            ),
                            description: item["description"]
                                .as_str()
                                .unwrap_or("No description")
                                .into(),
                            source: "GitHub".into(),
                            remote: String::new(),
                            installed: false,
                        }))
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| Err("GitHub verification stopped".into()))
                })
                .collect::<Vec<Result<Option<Package>, String>>>()
        });
        for result in checked {
            if let Some(package) = result? {
                results.push(package);
            }
        }
    }
    Ok(results)
}

pub fn install(package: &Package) -> Result<(), String> {
    if !valid_repo(&package.name) {
        return Err("Invalid GitHub repository".into());
    }
    for tool in ["git", "makepkg"] {
        if !super::available(tool) {
            return Err(format!(
                "GitHub builds require {tool}. Run Setup ArchEasy to install build tools."
            ));
        }
    }
    terminal(vec![
        "/usr/bin/bash".into(),
        "-c".into(),
        BUILD.into(),
        "archeasy-github".into(),
        package.name.clone(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    #[test]
    #[ignore = "uses the public GitHub API"]
    fn live_public_search() {
        let packages = search_github("fish").unwrap();
        eprintln!(
            "Verified GitHub build results: {:?}",
            packages.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        assert!(
            !packages.is_empty(),
            "A common Arch query must find buildable repositories"
        );
    }

    fn mock(
        count: usize,
        reply: impl Fn(&str) -> (u16, String) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for _ in 0..count {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut data = Vec::new();
                while !data.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    assert_eq!(stream.read(&mut byte).unwrap(), 1);
                    data.push(byte[0]);
                }
                let request = String::from_utf8(data).unwrap();
                let (status, body) = reply(&request);
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (url, worker)
    }

    #[test]
    fn checks_recipes_and_preserves_card_metadata() {
        let (url, server) = mock(4, |request| {
            assert!(
                request
                    .to_lowercase()
                    .contains("authorization: bearer fixture-token")
            );
            if request.starts_with("GET /search/repositories?") {
                assert!(request.contains("q=fish+config+arch+PKGBUILD"));
                assert!(request.contains("per_page=20"));
                (200, serde_json::json!({"items": [
                    {"full_name":"danny/dotfiles", "description":"fish with starship", "stargazers_count":12},
                    {"full_name":"test/missing"}, {"full_name":"test/directory"},
                    {"full_name":"../../payload"}, {"full_name":"test/private", "private":true}
                ]}).to_string())
            } else if request.starts_with("GET /repos/danny/dotfiles/contents/PKGBUILD ") {
                (200, r#"{"type":"file","size":100}"#.into())
            } else if request.starts_with("GET /repos/test/directory/") {
                (200, r#"{"type":"dir"}"#.into())
            } else {
                (404, "{}".into())
            }
        });
        let packages = search_with(
            &client(Some("fixture-token")).unwrap(),
            &url,
            "fish config arch",
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(packages.len(), 1);
        let p = &packages[0];
        assert_eq!(
            (&*p.name, &*p.description, &*p.version),
            ("danny/dotfiles", "fish with starship", "★ 12")
        );
        assert_eq!(
            super::super::package_page(p).unwrap().0,
            "https://github.com/danny/dotfiles"
        );
        assert!(!p.installed);
    }

    #[test]
    fn respects_candidate_limit() {
        let (url, server) = mock(21, |request| {
            if request.starts_with("GET /search/") {
                (200, serde_json::json!({"items": (0..25).map(|i| serde_json::json!({"full_name":format!("test/repo{i}")})).collect::<Vec<_>>()}).to_string())
            } else {
                (200, r#"{"type":"file","size":10}"#.into())
            }
        });
        assert_eq!(
            search_with(&client(None).unwrap(), &url, "arch")
                .unwrap()
                .len(),
            20
        );
        server.join().unwrap();
    }

    #[test]
    fn reports_api_errors_and_rejects_bad_tokens() {
        for (code, body, message) in [
            (403, "{}", "limited"),
            (429, "{}", "limited"),
            (401, "{}", "rejected"),
            (200, "not json", "Unexpected"),
            (200, "{}", "Unexpected"),
        ] {
            let (url, server) = mock(1, move |_| (code, body.into()));
            assert!(
                search_with(&client(None).unwrap(), &url, "arch")
                    .unwrap_err()
                    .contains(message)
            );
            server.join().unwrap();
        }
        assert!(client(Some("token\r\ninjected: value")).is_err());
        for repo in [
            "../repo",
            "user/..",
            "-owner/repo",
            "user/repo/extra",
            "user/$(id)",
            "user/repo;id",
        ] {
            assert!(!valid_repo(repo));
        }
    }
}
