use anyhow::Result;
use directories::UserDirs;
use git2::{Cred, RemoteCallbacks, Repository};
use oauth2::http::header::ACCEPT;
use octocrab::Octocrab;
use secrecy::{ExposeSecret, SecretBox};

use crate::{
    common_cli::success, hermitgrab_error::DiscoverError, hermitgrab_info, info, success, warn,
};

pub fn clone_or_update_repo(repo: String, token: Option<&str>) -> Result<(), DiscoverError> {
    let hermit_dir = hermit_dir();
    if hermit_dir.exists() {
        info!("Updating existing repo at {}", hermit_dir.display());
        let repo = Repository::open(&hermit_dir)?;
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["main"], None, None)?;
    } else {
        info!("Cloning {} into {}", &repo, hermit_dir.display());
        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = token {
            callbacks.credentials(|_url, username_from_url, _allowed_types| {
                Cred::userpass_plaintext(username_from_url.unwrap_or("oauth2"), token)
            });
        } else {
            todo!("Implement SSH key authentication or other methods if token is not provided");
        }
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);
        let mut builder = git2::build::RepoBuilder::new();
        builder
            .fetch_options(fetch_opts)
            .branch("main")
            .clone(&repo, &hermit_dir)?;
        success!("Cloned repository to {}", hermit_dir.display());
    }
    Ok(())
}

fn hermit_dir() -> std::path::PathBuf {
    let user_dirs = UserDirs::new().expect("Could not get user directories");
    let dotfiles_dir = user_dirs.home_dir().join(".hermitgrab");
    dotfiles_dir
}

pub async fn discover_repo(create: bool) -> Result<(), DiscoverError> {
    hermitgrab_info!("Discovering dotfiles repository...");
    let dotfiles_dir = hermit_dir();
    if dotfiles_dir.exists() {
        info!(
            "Dotfiles directory already exists at {}",
            dotfiles_dir.display()
        );
        Repository::open(&dotfiles_dir)?;
        info!("Repository already initialized, skipping discovery.");
        return Ok(());
    }
    let pat = std::env::var("HERMITGRAB_GITHUB_TOKEN");
    let (octocrab, token) = if let Ok(token) = pat {
        info!("Using HERMITGRAB_GITHUB_TOKEN from environment");
        let octocrab = Octocrab::builder();
        (octocrab.personal_token(token.clone()).build()?, token)
    } else {
        let client_id = SecretBox::new("Ov23liA8rPwqTP9hUCtL".to_string().into_boxed_str());
        info!("No HERMITGRAB_GITHUB_TOKEN env found, using device authentication");
        let octocrab = Octocrab::builder()
            .base_uri("https://github.com")?
            .add_header(ACCEPT, "application/json".to_string())
            .build()?;
        let codes = octocrab
            .authenticate_as_device(&client_id, ["repo"])
            .await?;
        warn!(
            "Go to {} and enter code {}",
            codes.verification_uri, codes.user_code
        );
        let auth = codes.poll_until_available(&octocrab, &client_id).await?;
        info!(
            "Authentication successful, using token with scopes: {} token: {}",
            auth.scope.join(", "),
            auth.access_token.expose_secret()
        );
        let token = auth.clone().access_token.expose_secret().to_string();
        (Octocrab::builder().oauth(auth).build()?, token)
    };
    success("Authorization successful!");
    let my_repos = octocrab
        .current()
        .list_repos_for_authenticated_user()
        .type_("all")
        .sort("updated")
        .per_page(100)
        .send()
        .await?;

    let mut found_repos = vec![];
    for repo in my_repos {
        if let Some(ref topics) = repo.topics {
            if topics.iter().any(|t| t.to_lowercase() == "hermitgrab") {
                found_repos.push(repo.clone());
                continue;
            }
        }
        if repo.name == "dotfiles" {
            found_repos.push(repo.clone());
        }
    }

    if found_repos.is_empty() {
        if create {
            hermitgrab_info!("No HermitGrab repo found, creating new repository...");
            let repo_name = "dotfiles";
            let repo_create = serde_json::json!({
                "name": repo_name,
                "description": "Dotfiles managed by HermitGrab",
                "private": true,
                "topics": ["HermitGrab"]
            });
            let repo: octocrab::models::Repository =
                octocrab.post("/user/repos", Some(&repo_create)).await?;
            success!("Created repo: {:?}", repo.full_name);
            if let Some(clone_url) = &repo.clone_url {
                hermitgrab_info!("Cloning {}...", clone_url);
                clone_or_update_repo(clone_url.to_string(), Some(&token))?;
            } else {
                return Err(DiscoverError::NoGitCloneUrl(repo_name.to_string()));
            }
        } else {
            warn!("No HermitGrab repo found. Use --create to create one.");
        }
        return Ok(());
    }

    hermitgrab_info!("Found the following repositories:");
    for (i, repo) in found_repos.iter().enumerate() {
        info!("{}: {:?}", i + 1, repo.name);
    }

    let selected_repo = if found_repos.len() == 1 {
        &found_repos[0]
    } else {
        use std::io::{self, Write};
        print!("Select a repository to clone [1-{}]: ", found_repos.len());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let idx: usize = input.trim().parse().unwrap_or(1);
        if idx == 0 || idx > found_repos.len() {
            return Err(DiscoverError::InvalidInput(
                "Invalid selection, please select a valid repository number.".to_string(),
            ));
        }
        &found_repos[idx - 1]
    };

    if let Some(clone_url) = &selected_repo.clone_url {
        clone_or_update_repo(clone_url.to_string(), Some(&token))?;
    } else {
        return Err(DiscoverError::NoGitCloneUrl(selected_repo.name.to_string()));
    }
    Ok(())
}

pub fn create_local_repo() -> Result<()> {
    hermitgrab_info!("Creating empty local dotfiles repo (not yet implemented)");
    // TODO: Implement local repo creation
    Ok(())
}
