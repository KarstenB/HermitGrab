// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Arc;

use anyhow::Result;
use git2::{Cred, RemoteCallbacks, Repository};
use oauth2::http::header::ACCEPT;
use octocrab::Octocrab;
use secrecy::{ExposeSecret, SecretBox};

use crate::common_cli::success;
use crate::config::GlobalConfig;
use crate::hermitgrab_error::DiscoverError;
use crate::{hermitgrab_info, info, prompt, success, warn};

pub fn clone_or_update_repo(
    repo: &str,
    token: Option<&str>,
    global_config: &Arc<GlobalConfig>,
) -> Result<(), DiscoverError> {
    let hermit_dir = global_config.hermit_dir();
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
    if hermit_dir.exists() {
        info!("Updating existing repo at {}", hermit_dir.display());
        let repo = Repository::open(hermit_dir)?;
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["main"], Some(&mut fetch_opts), None)?;
    } else {
        info!("Cloning {} into {}", &repo, hermit_dir.display());
        let mut builder = git2::build::RepoBuilder::new();
        builder
            .fetch_options(fetch_opts)
            .branch("main")
            .clone(repo, hermit_dir)?;
        success!("Cloned repository to {}", hermit_dir.display());
    }
    Ok(())
}

pub async fn discover_repo_with_github(
    create: bool,
    token: Option<String>,
    global_config: &Arc<GlobalConfig>,
) -> Result<(), DiscoverError> {
    hermitgrab_info!("Discovering dotfiles repository...");
    let (octocrab, token) = if let Some(token) = token {
        let octocrab = Octocrab::builder().personal_token(token.clone()).build()?;
        (octocrab, token)
    } else {
        github_device_flow_auth().await?
    };
    let found_repos = github_find_hermitgrab_topic_repos(&octocrab).await?;

    if found_repos.is_empty() {
        if create {
            hermitgrab_info!("No HermitGrab repo found, creating new repository...");
            github_create_repo(octocrab, &token, global_config).await?;
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
        let input = prompt!("Select a repository to clone [1-{}]: ", found_repos.len())?;
        let idx: usize = input.trim().parse().unwrap_or(1);
        if idx == 0 || idx > found_repos.len() {
            return Err(DiscoverError::InvalidInput(
                "Invalid selection, please select a valid repository number.".to_string(),
            ));
        }
        &found_repos[idx - 1]
    };

    if let Some(clone_url) = &selected_repo.clone_url {
        clone_or_update_repo(clone_url.as_ref(), Some(&token), global_config)?;
    } else {
        return Err(DiscoverError::NoGitCloneUrl(selected_repo.name.to_string()));
    }
    Ok(())
}

async fn github_find_hermitgrab_topic_repos(
    octocrab: &Octocrab,
) -> Result<Vec<octocrab::models::Repository>, DiscoverError> {
    let my_repos = octocrab
        .current()
        .list_repos_for_authenticated_user()
        .type_("all")
        .sort("full_name")
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
    Ok(found_repos)
}

async fn github_create_repo(
    octocrab: Octocrab,
    token: &str,
    global_config: &Arc<GlobalConfig>,
) -> Result<(), DiscoverError> {
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
        clone_or_update_repo(clone_url.as_ref(), Some(token), global_config)?;
    } else {
        return Err(DiscoverError::NoGitCloneUrl(repo_name.to_string()));
    };
    Ok(())
}

async fn github_device_flow_auth() -> Result<(Octocrab, String), DiscoverError> {
    let client_id = SecretBox::new("Ov23liA8rPwqTP9hUCtL".to_string().into_boxed_str());
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
    success("Authentication successful");
    let token = auth.clone().access_token.expose_secret().to_string();
    Ok((Octocrab::builder().oauth(auth).build()?, token))
}

pub fn create_local_repo(global_config: &Arc<GlobalConfig>) -> Result<(), DiscoverError> {
    let hermit_dir = global_config.hermit_dir();
    if hermit_dir.exists() {
        warn!(
            "Dotfiles directory already exists at {}",
            hermit_dir.display()
        );
        return Err(DiscoverError::RepoAlreadyExists(
            global_config.hermit_dir().into(),
        ));
    }
    hermitgrab_info!(
        "Creating empty local dotfiles repo in {}",
        hermit_dir.display()
    );
    if let Some(hermit_parent) = hermit_dir.parent() {
        if !hermit_parent.exists() {
            std::fs::create_dir_all(hermit_parent)?;
        }
    }
    Repository::init(hermit_dir)?;
    success!("Initialized empty repository at {}", hermit_dir.display());
    info!("You can now add your dotfiles to this directory and commit them.");
    Ok(())
}
