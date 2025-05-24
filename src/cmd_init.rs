use anyhow::Result;

pub fn run(repo: String) -> Result<()> {
    println!("[hermitgrab] Cloning repo: {}", repo);
    use directories::UserDirs;
    use git2::Repository;
    let user_dirs = UserDirs::new().expect("Could not get user directories");
    let dotfiles_dir = user_dirs.home_dir().join(".hermitgrab/dotfiles");
    if dotfiles_dir.exists() {
        println!(
            "[hermitgrab] Updating existing repo at {}",
            dotfiles_dir.display()
        );
        let repo = Repository::open(&dotfiles_dir)?;
        let mut remote = repo.find_remote("origin")?;
        remote.fetch(&["main"], None, None)?;
    } else {
        println!("[hermitgrab] Cloning into {}", dotfiles_dir.display());
        Repository::clone(&repo, &dotfiles_dir)?;
    }
    Ok(())
}
