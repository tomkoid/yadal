use anyhow::{Context, Result};
use std::io;
use std::path::Path;
use tidlers::{auth::init::TidalAuth, client::TidalClient};

pub async fn load_or_authenticate(session_file: &Path, use_oauth2: bool) -> Result<TidalClient> {
    // try to load existing session
    if session_file.exists() {
        match load_and_refresh_session(session_file).await {
            Ok(client) => return Ok(client),
            Err(e) => {
                println!("failed to load session ({}), re-authenticating...\n", e);
            }
        }
    } else {
        println!("no session found. authenticating...\n");
    }

    authenticate(session_file, use_oauth2).await
}

async fn load_and_refresh_session(
    session_file: &Path,
) -> Result<TidalClient, Box<dyn std::error::Error>> {
    println!("loading session from {}...", session_file.display());

    let session_data = std::fs::read_to_string(session_file)?;
    let mut client = TidalClient::from_json(&session_data)?;

    let refreshed = client.refresh_access_token(false).await?;
    if refreshed {
        println!("token refreshed successfully\n");
        save_session(&client, session_file)?;
    } else {
        println!("using existing session\n");
    }

    Ok(client)
}

pub async fn authenticate(session_file: &Path, use_oauth2: bool) -> Result<TidalClient> {
    let auth = if use_oauth2 {
        TidalAuth::with_oauth()
    } else {
        TidalAuth::with_pkce()
    };
    let mut client = TidalClient::new(&auth);

    if use_oauth2 {
        if client.waiting_for_oauth_login() {
            let oauth_response = client
                .get_oauth_link()
                .await
                .context("Failed to get OAuth link")?;

            println!(
                "please visit and sign in: https://{:<24}",
                oauth_response.verification_uri_complete
            );
            println!("waiting for authorization...");

            client
                .wait_for_oauth(
                    &oauth_response.device_code,
                    oauth_response.expires_in,
                    oauth_response.interval,
                    None,
                )
                .await
                .context("OAuth flow failed")?;
        }
    } else {
        let login_url = client
            .initiate_pkce_login()
            .context("Failed to initiate PKCE login")?;
        println!("please visit and sign in: {}", login_url);
        println!("after browser redirect, paste the full redirect URL:");

        let mut redirect_url = String::new();
        io::stdin()
            .read_line(&mut redirect_url)
            .context("Failed to read redirect URL from stdin")?;

        client
            .finish_pkce_login(redirect_url.trim())
            .await
            .context("PKCE flow failed")?;
    }

    println!("authorization successful!\n");

    // get user info
    client
        .refresh_user_info()
        .await
        .context("Failed to get user info")?;

    if let Some(user) = &client.user_info {
        println!("logged in as: {}", user.username);
    }

    // get subscription info
    match client.subscription().await {
        Ok(sub) => {
            println!("subscription: {}\n", sub.subscription.subscription_type);
        }
        Err(e) => {
            println!("could not fetch subscription: {}\n", e);
        }
    }

    // save session
    save_session(&client, session_file)?;

    Ok(client)
}

fn save_session(client: &TidalClient, session_file: &Path) -> Result<()> {
    if let Some(parent) = session_file.parent() {
        std::fs::create_dir_all(parent).context("Failed to create session directory")?;
    }
    let session_json = client.get_json();
    std::fs::write(session_file, session_json).context("Failed to save session")?;
    println!("session saved to {}", session_file.display());
    Ok(())
}
