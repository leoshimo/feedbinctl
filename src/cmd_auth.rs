use anyhow::{Context, Result};
use keyring::Entry;
use rpassword::prompt_password;
use std::io::{self, Write};

pub async fn login() -> Result<()> {
    let mut username = String::new();
    print!("Feedbin username: ");
    io::stdout().flush().context("failed to flush stdout")?;
    io::stdin()
        .read_line(&mut username)
        .context("failed to read username")?;
    let username = username.trim();

    let password = prompt_password("Feedbin password: ")?;

    let credentials = format!("{}:{}", username, password);
    let entry = Entry::new("feedbinctl", "feedbin")
        .context("failed to open keyring entry")?;
    entry
        .set_password(&credentials)
        .context("failed to store credentials in keyring")?;

    println!("Credentials stored in keyring");
    Ok(())
}

pub async fn logout() -> Result<()> {
    let entry = Entry::new("feedbinctl", "feedbin")
        .context("failed to open keyring entry")?;
    match entry.delete_password() {
        Ok(_) => println!("Credentials removed from keyring"),
        Err(err) => println!("No credentials found ({err})"),
    }
    Ok(())
}
