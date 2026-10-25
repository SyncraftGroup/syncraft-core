use std::path::Path;

use super::sync;
use crate::client::auth::{
    Agent, AuthenticateRequest, AuthenticateResponse, RefreshRequest, RefreshResponse,
    RegisterRequest, RegisterResponse, is_valid_minecraft_username, offline_uuid,
};
use crate::client::config::{Account, client_config};
use crate::client::message::{MessageKey, format_version_info, translate};
use console_info::{Ui, Validation};
use java_detect::JavaInstallation;
use syncraft_core::Error;
use syncraft_core::download::UiProgress;
use syncraft_core::install::Installer;
use syncraft_core::launcher::{LaunchContext, Launcher};
use syncraft_core::paths::MinecraftPaths;

fn get_link(ui: &Ui) -> Result<String, Error> {
    let config_m = client_config();
    if let Some(link) = config_m.get().syncraft_link {
        ui.step(&link);
        if ui.confirm(&translate(&MessageKey::ConfirmLink))? {
            Ok(link)
        } else {
            config_m.update(|cfg| cfg.syncraft_link = None)?;
            get_link(ui)
        }
    } else {
        let link = ui.input(&translate(&MessageKey::InputLink))?;
        let link = link.trim().trim_matches('/').to_string();
        config_m.update(|cfg| cfg.syncraft_link = Some(link.clone()))?;
        Ok(link)
    }
}

fn get_java(ui: &Ui, recommended_version: Option<u8>) -> Result<JavaInstallation, Error> {
    let config_m = client_config();
    if let Some(java) = config_m.get().java {
        ui.step(&java.to_string());
        if ui.confirm(&translate(&MessageKey::ConfirmJavaPath))? {
            Ok(java)
        } else {
            config_m.update(|cfg| cfg.java = None)?;
            get_java(ui, recommended_version)
        }
    } else {
        let javas = java_detect::detect();
        let selected = ui.select(
            &translate(&MessageKey::SelectJava(recommended_version)),
            javas,
        )?;
        config_m.update(|cfg| cfg.java = Some(selected.clone()))?;
        Ok(selected)
    }
}

async fn get_account(ui: &Ui, link: &str) -> Result<Option<Account>, Error> {
    let config_m = client_config();
    if let Some(account) = config_m.get().account {
        ui.step(&account.to_string());
        if ui.confirm(&translate(&MessageKey::ConfirmAuth))? {
            Ok(Some(account))
        } else {
            config_m.update(|cfg| cfg.account = None)?;
            Ok(None)
        }
    } else {
        let option_offline = translate(&MessageKey::OfflineMethod);
        let option_login = translate(&MessageKey::SyncraftMethodLogin);
        let option_signup = translate(&MessageKey::SyncraftMethodSignUp);
        let options = vec![&option_offline, &option_login, &option_signup];
        let selected = ui.select(&translate(&MessageKey::SelectAuthMethod), options)?;

        if selected == &option_offline {
            let username = get_username(ui)?;
            let uuid = offline_uuid(&username);
            let account = Account::Offline { username, uuid };
            config_m.update(|cfg| cfg.account = Some(account.clone()))?;
            Ok(Some(account))
        } else if selected == &option_login {
            let email = ui.input(&translate(&MessageKey::InputEmail))?;
            let password = ui.password(&translate(&MessageKey::InputPassword), |_: &str| {
                Ok(Validation::Valid)
            })?;
            let client = reqwest::Client::new();
            let req = AuthenticateRequest {
                agent: Agent {
                    name: "Minecraft".into(),
                    version: 1,
                },
                username: email.clone(),
                password: password,
            };
            let login_link = format!("{link}/yggdrasil/authserver/authenticate");
            let resp = client.post(&login_link).json(&req).send().await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(Error::Other(resp.text().await?));
            }
            let auth: AuthenticateResponse = resp.json().await?;
            let profile = auth
                .selected_profile
                .or(auth.available_profiles.into_iter().next())
                .ok_or(Error::Other("no profile in auth response".into()))?;
            let account = Account::Syncraft {
                username: profile.name,
                uuid: profile.id,
                email,
                client_token: auth.client_token,
                access_token: auth.access_token,
            };
            config_m.update(|cfg| cfg.account = Some(account.clone()))?;
            Ok(Some(account))
        } else if selected == &option_signup {
            let username = ui.input(&translate(&MessageKey::InputUsername))?;
            let email = ui.input(&translate(&MessageKey::InputEmail))?;
            let password =
                ui.password(&translate(&MessageKey::InputPassword), |password: &str| {
                    if password.trim().is_empty() {
                        return Ok(Validation::Invalid(
                            translate(&MessageKey::EmptyPassword).into(),
                        ));
                    }
                    if password.len() < 8 {
                        return Ok(Validation::Invalid(
                            translate(&MessageKey::PasswordTooShort).into(),
                        ));
                    }
                    if password.chars().any(|c| c.is_control()) {
                        return Ok(Validation::Invalid(
                            translate(&MessageKey::InvalidPassword).into(),
                        ));
                    }
                    Ok(Validation::Valid)
                })?;
            let password_confirm = ui
                .password(&translate(&MessageKey::InputPasswordAgain), |_: &str| {
                    Ok(Validation::Valid)
                })?;
            if password != password_confirm {
                ui.warn(&translate(&MessageKey::PasswordUnmatch));
                return Ok(None);
            }
            let client = reqwest::Client::new();
            let req = RegisterRequest {
                username: email,
                password,
                character_name: username,
            };
            let signup_link = format!("{link}/yggdrasil/register");
            let resp = client.post(&signup_link).json(&req).send().await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(Error::Other(resp.text().await?));
            }
            let result: RegisterResponse = resp.json().await?;
            ui.ok(&result.id);
            ui.info(&translate(&MessageKey::VerifyMail));
            Ok(None)
        } else {
            unreachable!();
        }
    }
}

fn get_username(ui: &Ui) -> Result<String, Error> {
    let username = ui.input(&translate(&MessageKey::InputUsername))?;
    if !is_valid_minecraft_username(&username) {
        ui.warn(&translate(&MessageKey::InvalidUsername));
        return get_username(ui);
    }
    Ok(username)
}

async fn refresh_token(
    link: &str,
    access_token: &str,
    client_token: &str,
) -> Result<(String, String), Error> {
    let client = reqwest::Client::new();
    let req = RefreshRequest {
        access_token: access_token.to_string(),
        client_token: client_token.to_string(),
        selected_profile: None,
    };
    let refresh_link = format!("{link}/yggdrasil/authserver/refresh");
    let resp = client.post(&refresh_link).json(&req).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Other(resp.text().await?));
    }
    let result: RefreshResponse = resp.json().await?;
    Ok((result.access_token, result.client_token))
}

async fn refresh_or_reauthenticate(
    ui: &Ui,
    link: &str,
    account: Account,
) -> Result<Account, Error> {
    match account {
        Account::Syncraft {
            username,
            uuid,
            email,
            client_token,
            access_token,
        } => match refresh_token(link, &access_token, &client_token).await {
            Ok((new_access_token, new_client_token)) => {
                let account = Account::Syncraft {
                    username,
                    uuid,
                    email,
                    client_token: new_client_token,
                    access_token: new_access_token,
                };
                client_config().update(|cfg| cfg.account = Some(account.clone()))?;
                Ok(account)
            }
            // A network failure says nothing about the stored token; keep it.
            Err(e @ Error::Network(_)) => Err(e),
            Err(_) => {
                ui.warn(&translate(&MessageKey::TokenExpired));
                client_config().update(|cfg| cfg.account = None)?;
                loop {
                    if let Some(new_account) = get_account(ui, link).await? {
                        return Ok(new_account);
                    }
                }
            }
        },
        offline => Ok(offline),
    }
}

async fn upload_skin_if_changed(
    ui: &Ui,
    link: &str,
    uuid: &str,
    access_token: &str,
    game_dir: &Path,
) -> Result<(), Error> {
    let skin_path = game_dir.join("skin.png");
    if !skin_path.exists() {
        return Ok(());
    }

    let data = tokio::fs::read(&skin_path).await?;
    let file_hash = blake3::hash(&data).to_hex().to_string();

    let option_default = translate(&MessageKey::SkinModelClassic);
    let option_slim = translate(&MessageKey::SkinModelSlim);
    let options = vec![&option_default, &option_slim];
    let selected = ui.select(&translate(&MessageKey::SelectSkinType), options)?;
    let variant = if selected == &option_slim {
        "slim"
    } else {
        "default"
    };

    // Re-upload when either the image or the selected model changed.
    let cache_key = format!("{file_hash}:{variant}");
    if client_config().get().skin_hash.as_deref() == Some(&cache_key) {
        return Ok(());
    }

    ui.step(&translate(&MessageKey::UploadingSkin));

    // The endpoint expects `multipart/form-data` with a `model` field and a
    // `file` field, authenticated with the access token.
    let boundary = format!("----syncraft{file_hash}");
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{variant}\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let url = format!("{link}/api/user/profile/{uuid}/skin");
    let client = reqwest::Client::new();
    let resp = client
        .put(&url)
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .header("Authorization", format!("Bearer {access_token}"))
        .body(body)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "skin upload failed: {} - {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }

    client_config().update(|cfg| cfg.skin_hash = Some(cache_key))?;
    Ok(())
}

pub async fn run() -> Result<(), Error> {
    console_info::configure_inquire();
    let ui = Ui::get();

    syncraft_core::set_config(".core.json");
    crate::client::config::init_client_config(".client.json");

    ui.echo(&translate(&MessageKey::Entrance));

    let link = get_link(&ui)?;

    ui.step(&translate(&MessageKey::Connecting(&link)));

    // The game directory is independent of the manifest, so the cached copy can
    // be used for a conditional request before we know the server version.
    let game_dir = std::env::current_dir()?.join(syncraft_core::config().get().game_dir);
    let cached_manifest = sync::cached_manifest(&game_dir);

    let manifest = if let Ok(manifest) = sync::fetch_manifest(&link, cached_manifest.as_ref()).await
    {
        manifest
    } else {
        ui.error(&translate(&MessageKey::LinkInvalid));
        return Ok(());
    };

    ui.plain(&format_version_info(&manifest.version));

    let client_state = client_config().get();
    let saved_hash = client_state.version_hash.clone();
    let saved_commit = client_state.commit.clone();
    if let Some(ref old_hash) = saved_hash
        && *old_hash != manifest.version_hash
        && let Some(old_commit) = saved_commit.as_deref()
        && !manifest.commit.is_empty()
    {
        // Changelogs are keyed by git commit; `version_hash` is a content hash.
        ui.step(&translate(&MessageKey::UpdateLog));
        if let Ok(updates) = sync::fetch_update_info(&link, old_commit).await {
            for update in &updates {
                ui.plain(&format!("  {}", update.message));
            }
        }
    }

    let paths = MinecraftPaths::new(manifest.version.file_id(), manifest.version.cache_key())?;
    let progress = UiProgress::empty();

    let version = Installer::meta_vanilla(&manifest.version, &paths, Some(&progress)).await?;
    let java = get_java(&ui, version.java_version.clone())?;
    let version =
        Installer::restore_or_install(&manifest.version, version, &paths, Some(&progress)).await?;
    let account = loop {
        if let Some(account) = get_account(&ui, &link).await? {
            break account;
        }
    };
    let account = refresh_or_reauthenticate(&ui, &link, account).await?;

    ui.step(&translate(&MessageKey::Syncing));
    let plan = sync::plan_sync(&manifest, paths.root()).await?;
    sync::download_files(&link, &manifest, &plan, &progress).await?;
    ui.ok(&translate(&MessageKey::SyncDone));

    // Mark this version as applied only once the files are actually in place;
    // otherwise a failed sync would swallow the changelog on the next run.
    client_config().update(|cfg| {
        cfg.version_hash = Some(manifest.version_hash.clone());
        cfg.commit = Some(manifest.commit.clone());
    })?;

    if let Account::Syncraft {
        uuid, access_token, ..
    } = &account
        && let Err(e) = upload_skin_if_changed(&ui, &link, uuid, access_token, paths.root()).await
    {
        // A failed skin upload must not prevent the game from starting.
        ui.warn(&translate(&MessageKey::SkinUploadFailed(&e.to_string())));
    }

    let (username, uuid, access_token, auth_server) = match &account {
        Account::Offline { username, uuid } => {
            (username.clone(), uuid.clone(), "0".to_string(), None)
        }
        Account::Syncraft {
            username,
            uuid,
            access_token,
            ..
        } => (
            username.clone(),
            uuid.clone(),
            access_token.clone(),
            Some(format!("{link}/yggdrasil")),
        ),
    };

    let launch_ctx = LaunchContext {
        java_path: java.path.clone(),
        username,
        uuid,
        access_token,
        launcher_name: "syncraft".into(),
        launcher_version: "0.0.1".into(),
        auth_server,
    };

    ui.step(&translate(&MessageKey::Launching));
    let mut child = Launcher::launch(version, launch_ctx, &paths, None).await?;
    child
        .wait()
        .map_err(|e| Error::Other(format!("minecraft process error: {e}")))?;

    Ok(())
}
