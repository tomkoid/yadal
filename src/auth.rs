use anyhow::{Context, Result};
use std::path::Path;
use tidlers::{auth::init::TidalAuth, client::TidalClient};

pub async fn load_or_authenticate(session_file: &Path) -> Result<TidalClient> {
    // try to load existing session
    if session_file.exists() {
        println!("loading session from {}...", session_file.display());
        match std::fs::read_to_string(session_file) {
            Ok(session_data) => {
                match TidalClient::from_json(&session_data) {
                    Ok(mut client) => {
                        // try to refresh token
                        match client.refresh_access_token(false).await {
                            Ok(refreshed) => {
                                if refreshed {
                                    println!("token refreshed successfully\n");
                                    save_session(&client, session_file)?;
                                } else {
                                    println!("using existing session\n");
                                }
                                return Ok(client);
                            }
                            Err(e) => {
                                println!("failed to refresh token: {}", e);
                                println!("re-authenticating...\n");
                            }
                        }
                    }
                    Err(e) => {
                        println!("failed to parse session: {}", e);
                        println!("re-authenticating...\n");
                    }
                }
            }
            Err(e) => {
                println!("failed to read session file: {}", e);
                println!("authenticating...\n");
            }
        }
    } else {
        println!("no session found. authenticating...\n");
    }

    authenticate(session_file).await
}

pub async fn authenticate(session_file: &Path) -> Result<TidalClient> {
    // create new client with OAuth
    let auth = TidalAuth::with_oauth();
    let mut client = TidalClient::new(&auth);

    // handle OAuth flow
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

        println!("authorization successful!\n");
    }

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
