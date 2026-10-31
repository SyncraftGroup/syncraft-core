use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::progress::Progress;
use syncraft_protocol::{
    MANIFEST_SCHEMA, Manifest, SyncMode, UpdateInfo, manifest_signing_payload,
};

use crate::client::config::client_config;

/// Fallback number of parallel downloads when neither the client config nor the
/// server manifest specifies one.
const DEFAULT_DOWNLOAD_CONCURRENCY: usize = 4;

/// Local record of what this client applied last time.
const STATE_FILE: &str = ".syncraft-state.json";

/// Files the client owns itself; they must never be removed by a sync, even
/// when a mirror (strict) directory would otherwise delete everything else.
const PROTECTED_TOP_LEVEL: &[&str] = &[
    ".core.json",
    ".client.json",
    STATE_FILE,
    "authlib-injector.jar",
];

pub async fn fetch_manifest(base: &str, cached: Option<&Manifest>) -> Result<Manifest, Error> {
    let url = format!("{base}/manifest");
    let client = reqwest::Client::new();

    let mut request = client.get(&url);
    if let Some(cached) = cached {
        request = request.header(
            reqwest::header::IF_NONE_MATCH,
            format!("\"{}\"", cached.version_hash),
        );
    }

    let resp = request.send().await.map_err(Error::Network)?;

    // Unchanged since the last run: reuse the cached copy instead of
    // downloading (potentially large) JSON again.
    let manifest = if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        cached.cloned().ok_or_else(|| {
            Error::Other("server returned 304 but no cached manifest is available".into())
        })?
    } else {
        if !resp.status().is_success() {
            return Err(Error::Other(format!(
                "failed to fetch manifest: {}",
                resp.status()
            )));
        }
        let manifest: Manifest = resp.json().await.map_err(|e| Error::Other(e.to_string()))?;

        if manifest.schema != MANIFEST_SCHEMA {
            return Err(Error::Other(format!(
                "incompatible manifest schema {} (this client supports {}); update the client or server",
                manifest.schema, MANIFEST_SCHEMA
            )));
        }
        manifest
    };

    verify_manifest(&manifest)?;
    Ok(manifest)
}

fn short_key(key: &str) -> &str {
    &key[..key.len().min(16)]
}

/// Verify the optional detached signature and pin the server key on first use.
///
/// With plain HTTP a signature is the only way to notice that the manifest was
/// modified by a proxy; it does not hide the manifest's contents.
fn verify_manifest(manifest: &Manifest) -> Result<(), Error> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let pinned = client_config().get().server_key;

    let Some(signature) = &manifest.signature else {
        if pinned.is_some() {
            return Err(Error::Other(
                "the server stopped signing its manifest; refusing to sync.\n\n  Remove \"server_key\" from .client.json if this is expected."
                    .into(),
            ));
        }
        return Ok(());
    };

    if signature.algorithm != "ed25519" {
        return Err(Error::Other(format!(
            "unsupported manifest signature algorithm: {}",
            signature.algorithm
        )));
    }

    let key: [u8; 32] = hex::decode(&signature.key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::Other("malformed manifest signing key".into()))?;
    let verifying = VerifyingKey::from_bytes(&key)
        .map_err(|_| Error::Other("invalid manifest signing key".into()))?;

    let signature_bytes: [u8; 64] = hex::decode(&signature.value)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| Error::Other("malformed manifest signature".into()))?;
    let signature_value = Signature::from_bytes(&signature_bytes);

    verifying
        .verify_strict(
            &manifest_signing_payload(&manifest.version_hash),
            &signature_value,
        )
        .map_err(|_| Error::Other("manifest signature verification failed".into()))?;

    match pinned {
        Some(pinned) if pinned != signature.key => Err(Error::Other(format!(
            "the server signing key changed (pinned {}, got {}).\n\n  If you trust the server, remove \"server_key\" from .client.json.",
            short_key(&pinned),
            short_key(&signature.key)
        ))),
        None => {
            // Trust on first use: remember the key so a later change is visible.
            client_config().update(|cfg| cfg.server_key = Some(signature.key.clone()))?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub async fn fetch_update_info(base: &str, since_hash: &str) -> Result<Vec<UpdateInfo>, Error> {
    let url = format!("{base}/updateInfo/{since_hash}");
    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await.map_err(Error::Network)?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "failed to fetch update info: {}",
            resp.status()
        )));
    }
    resp.json().await.map_err(|e| Error::Other(e.to_string()))
}

/// What this client applied during its last successful sync.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub version_hash: String,
    #[serde(default)]
    pub managed: BTreeSet<PathBuf>,
    /// Last manifest, used for conditional requests (`If-None-Match`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Manifest>,
}

impl SyncState {
    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<(), Error> {
        std::fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
    }
}

/// Cached manifest of the last successful sync, if any.
pub fn cached_manifest(game_dir: &Path) -> Option<Manifest> {
    SyncState::load(&game_dir.join(STATE_FILE)).manifest
}

#[derive(Debug)]
pub struct SyncPlan {
    pub to_download: HashMap<blake3::Hash, Vec<DownloadAction>>,
    pub to_delete: Vec<PathBuf>,
    pub game_dir: PathBuf,
}

#[derive(Debug)]
pub struct DownloadAction {
    pub rel_path: PathBuf,
    pub dest: PathBuf,
}

/// Manifest keys come from the server.  Refuse anything that could escape the
/// game directory (`..`, absolute paths, Windows prefixes).
fn is_safe_relative_path(path: &Path) -> bool {
    use std::path::Component;

    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), Error> {
    for rel_path in manifest.files.keys() {
        if !is_safe_relative_path(rel_path) {
            return Err(Error::Other(format!(
                "manifest contains an unsafe path: {}",
                rel_path.display()
            )));
        }
    }
    Ok(())
}

/// Top-level entries we never manage (client state, injected jar).
fn is_protected(game_dir: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(game_dir) else {
        return true;
    };
    relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|first| PROTECTED_TOP_LEVEL.contains(&first))
}

pub async fn plan_sync(manifest: &Manifest, game_dir: &Path) -> Result<SyncPlan, Error> {
    validate_manifest(manifest)?;

    let mut to_download: HashMap<blake3::Hash, Vec<DownloadAction>> = HashMap::new();
    let mut to_delete: BTreeSet<PathBuf> = BTreeSet::new();

    let existing_paths = walk_game_dir(game_dir).await?;
    let state = SyncState::load(&game_dir.join(STATE_FILE));

    for (rel_path, sync_file) in &manifest.files {
        let dest = game_dir.join(rel_path);
        match sync_file.mode {
            SyncMode::Forbidden => {
                if dest.exists() {
                    to_delete.insert(dest);
                }
            }
            SyncMode::Strict | SyncMode::Minimal => {
                if let Some(expected_state) = &sync_file.state {
                    let need_download = if dest.exists() {
                        !expected_state.matches(&dest).await.unwrap_or(false)
                    } else {
                        true
                    };
                    if need_download {
                        to_download
                            .entry(expected_state.hash)
                            .or_default()
                            .push(DownloadAction {
                                rel_path: rel_path.clone(),
                                dest,
                            });
                    }
                }
            }
            SyncMode::IfMissing => {
                if let Some(expected_state) = &sync_file.state
                    && !dest.exists()
                {
                    to_download
                        .entry(expected_state.hash)
                        .or_default()
                        .push(DownloadAction {
                            rel_path: rel_path.clone(),
                            dest,
                        });
                }
            }
        }
    }

    let trim_path =
        |p: &PathBuf| -> PathBuf { p.strip_prefix(game_dir).unwrap_or(p).to_path_buf() };

    let manifest_paths: HashSet<PathBuf> = manifest.files.keys().cloned().collect();
    let strict_dirs: HashSet<PathBuf> = manifest
        .files
        .iter()
        .filter(|(_, f)| f.mode == SyncMode::Strict)
        .filter_map(|(p, _)| p.parent().map(|d| d.to_path_buf()))
        .collect();

    // 1. Files that were managed last time but are gone from the manifest
    //    (removed on the server, or filtered out by the mod filter). Without
    //    this, `minimal` directories would keep stale mods forever.
    for managed in &state.managed {
        if manifest_paths.contains(managed) || !is_safe_relative_path(managed) {
            continue;
        }
        let dest = game_dir.join(managed);
        if dest.exists() && !is_protected(game_dir, &dest) {
            to_delete.insert(dest);
        }
    }

    // 2. Unmanaged files inside strict (mirrored) directories.
    for existing_path in &existing_paths {
        let rel = trim_path(existing_path);
        if manifest_paths.contains(&rel) || !is_safe_relative_path(&rel) {
            continue;
        }
        if is_protected(game_dir, existing_path) {
            continue;
        }
        if rel.starts_with("libraries")
            || rel.starts_with("assets")
            || rel.starts_with("versions")
            || rel.starts_with("natives")
        {
            continue;
        }
        let mut should_delete = false;
        let mut ancestor = rel.parent();
        while let Some(dir) = ancestor {
            if strict_dirs.contains(dir) {
                should_delete = true;
                break;
            }
            ancestor = dir.parent();
        }
        if should_delete {
            to_delete.insert(existing_path.clone());
        }
    }

    Ok(SyncPlan {
        to_download,
        to_delete: to_delete.into_iter().collect(),
        game_dir: game_dir.to_path_buf(),
    })
}

fn download_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("syncraft-core/0.1")
            .http1_only()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build download client")
    })
}

pub async fn download_files(
    base: &str,
    manifest: &Manifest,
    plan: &SyncPlan,
    progress: &dyn Progress,
) -> Result<(), Error> {
    progress.set_total(plan.to_download.len() as u64);

    // Client override > manifest hint > default.
    let concurrency = client_config()
        .get()
        .download_concurrency
        .or(manifest.download_concurrency.map(usize::from))
        .unwrap_or(DEFAULT_DOWNLOAD_CONCURRENCY)
        .clamp(1, 16);

    let client = download_client();

    // Build the futures in a loop and let `buffer_unordered` enforce the
    // concurrency limit. Using `stream::iter(..).map(..)` here trips a
    // higher-ranked-lifetime error once the sink is a `&dyn Progress`.
    let mut pending = Vec::with_capacity(plan.to_download.len());
    for (hash, actions) in &plan.to_download {
        pending.push(download_one(&client, base, hash, actions, progress));
    }

    let mut downloads = futures::stream::iter(pending).buffer_unordered(concurrency);
    while let Some(result) = downloads.next().await {
        result?;
        progress.overall_inc();
    }

    for path in &plan.to_delete {
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            tokio::fs::remove_file(path).await?;
        }
    }
    prune_empty_dirs(&plan.to_delete, &plan.game_dir).await;

    // Record what we manage so the next run can delete files the server
    // dropped (see step 1 in `plan_sync`).
    let state = SyncState {
        version_hash: manifest.version_hash.clone(),
        managed: manifest
            .files
            .iter()
            .filter(|(_, file)| file.mode != SyncMode::Forbidden)
            .map(|(path, _)| path.clone())
            .collect(),
        manifest: Some(manifest.clone()),
    };
    state.save(&plan.game_dir.join(STATE_FILE))?;

    Ok(())
}

async fn download_one(
    client: &reqwest::Client,
    base: &str,
    expected_hash: &blake3::Hash,
    actions: &[DownloadAction],
    progress: &dyn Progress,
) -> Result<(), Error> {
    let hash_hex = expected_hash.to_hex();
    let url = format!("{base}/download/{hash_hex}");

    let name = actions
        .first()
        .and_then(|action| action.rel_path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    progress.file_start(name, 0);

    let response = client.get(&url).send().await.map_err(Error::Network)?;

    // 409 means the server noticed its files changed mid-publish.
    if response.status() == reqwest::StatusCode::CONFLICT {
        progress.file_done(name);
        return Err(Error::Other(
            "the server is updating its files, please retry in a moment".into(),
        ));
    }

    let response = response.error_for_status().map_err(Error::Network)?;
    let total_size = response.content_length().unwrap_or(0);
    progress.file_total(name, total_size);

    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    let mut hasher = blake3::Hasher::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(Error::Network)?;
        hasher.update(&chunk);
        bytes.extend_from_slice(&chunk);
        progress.file_inc(name, chunk.len() as u64);
    }

    let actual_hash = hasher.finalize();
    if actual_hash != *expected_hash {
        progress.file_done(name);
        return Err(Error::Other(format!(
            "hash mismatch for {}: expected {}, got {}",
            name,
            hash_hex,
            actual_hash.to_hex()
        )));
    }

    for action in actions {
        if let Some(parent) = action.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Write then rename, so an interrupted download never leaves a
        // truncated file behind that still looks up-to-date.
        let file_name = action
            .dest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file");
        let temp_path = action
            .dest
            .with_file_name(format!("{file_name}.syncraft-part"));
        tokio::fs::write(&temp_path, &bytes).await?;
        tokio::fs::rename(&temp_path, &action.dest).await?;
    }

    progress.file_done(name);
    Ok(())
}

/// Remove directories that became empty after deleting files, bottom-up.
async fn prune_empty_dirs(deleted: &[PathBuf], game_dir: &Path) {
    let mut candidates: BTreeSet<PathBuf> = deleted
        .iter()
        .filter_map(|path| path.parent().map(|parent| parent.to_path_buf()))
        .collect();

    while let Some(dir) = candidates.pop_first() {
        if dir == game_dir || !dir.starts_with(game_dir) {
            continue;
        }
        let empty = match tokio::fs::read_dir(&dir).await {
            Ok(mut entries) => entries.next_entry().await.ok().flatten().is_none(),
            Err(_) => false,
        };
        if empty
            && tokio::fs::remove_dir(&dir).await.is_ok()
            && let Some(parent) = dir.parent()
        {
            candidates.insert(parent.to_path_buf());
        }
    }
}

async fn walk_game_dir(game_dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut paths = Vec::new();
    if !tokio::fs::try_exists(game_dir).await? {
        return Ok(paths);
    }
    let mut dirs = vec![game_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if entry.file_type().await?.is_dir() {
                dirs.push(path);
            } else {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use syncraft_protocol::{Loader, ManifestSignature, SyncFile, VersionInfo};

    fn manifest_with(files: Vec<(PathBuf, SyncMode)>) -> Manifest {
        let mut map = BTreeMap::new();
        for (path, mode) in files {
            map.insert(path, SyncFile { state: None, mode });
        }
        Manifest {
            schema: MANIFEST_SCHEMA,
            version_hash: "test".into(),
            commit: String::new(),
            version: VersionInfo {
                name: "test".into(),
                id: "test".into(),
                vanilla_version: "1.20.1".into(),
                loader: Loader::Vanilla,
            },
            download_concurrency: None,
            signature: None,
            files: map,
        }
    }

    fn temp_game_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn manifest_signature_is_verified_and_pinned() {
        use crate::client::config::{client_config, init_client_config};
        use ed25519_dalek::{Signer, SigningKey};

        let dir = std::env::temp_dir().join("syncraft_test_client_signature");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        init_client_config(dir.join(".client.json"));

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let mut manifest = manifest_with(vec![(PathBuf::from("mods/a.jar"), SyncMode::Strict)]);

        let sign = |manifest: &Manifest, key: &SigningKey| ManifestSignature {
            algorithm: "ed25519".into(),
            key: hex::encode(key.verifying_key().as_bytes()),
            value: hex::encode(
                key.sign(&manifest_signing_payload(&manifest.version_hash))
                    .to_bytes(),
            ),
        };

        // Unsigned manifest is accepted before any key is pinned.
        verify_manifest(&manifest).expect("unsigned manifest is allowed before pinning");

        // A valid signature pins the key.
        manifest.signature = Some(sign(&manifest, &signing_key));
        verify_manifest(&manifest).expect("signature must verify");
        assert_eq!(
            client_config().get().server_key.as_deref(),
            manifest.signature.as_ref().map(|s| s.key.as_str())
        );

        // Tampering with the content invalidates the signature.
        let mut tampered = manifest.clone();
        tampered.version_hash = "f".repeat(64);
        assert!(verify_manifest(&tampered).is_err());

        // Signature stripping is refused once a key is pinned.
        let mut stripped = manifest.clone();
        stripped.signature = None;
        assert!(verify_manifest(&stripped).is_err());

        // A different key is refused instead of silently re-pinning.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let mut rotated = manifest.clone();
        rotated.signature = Some(sign(&rotated, &other));
        assert!(verify_manifest(&rotated).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn plan_sync_rejects_path_traversal() {
        let dir = temp_game_dir("syncraft_test_plan_sync_traversal");

        for bad in ["../evil.txt", "/etc/passwd", "mods/../../evil.txt"] {
            let manifest = manifest_with(vec![(PathBuf::from(bad), SyncMode::Forbidden)]);
            let err = plan_sync(&manifest, &dir)
                .await
                .expect_err("traversal path must be rejected");
            assert!(err.to_string().contains("unsafe path"), "got: {err}");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn plan_sync_accepts_normal_paths() {
        let dir = temp_game_dir("syncraft_test_plan_sync_ok");
        let manifest = manifest_with(vec![(PathBuf::from("mods/a.jar"), SyncMode::Forbidden)]);
        plan_sync(&manifest, &dir)
            .await
            .expect("safe paths must be accepted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn plan_sync_deletes_files_removed_from_manifest() {
        let dir = temp_game_dir("syncraft_test_plan_sync_removed");
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("mods/stale.jar"), b"old").unwrap();
        std::fs::write(dir.join("mods/keep.jar"), b"keep").unwrap();

        // Previous run managed both files.
        SyncState {
            version_hash: "old".into(),
            managed: [
                PathBuf::from("mods/stale.jar"),
                PathBuf::from("mods/keep.jar"),
            ]
            .into_iter()
            .collect(),
            manifest: None,
        }
        .save(&dir.join(STATE_FILE))
        .unwrap();

        // Server now only provides `keep.jar` (minimal: extras are left alone).
        let manifest = manifest_with(vec![(PathBuf::from("mods/keep.jar"), SyncMode::Minimal)]);
        let plan = plan_sync(&manifest, &dir).await.unwrap();

        let deleted: Vec<PathBuf> = plan.to_delete;
        assert_eq!(deleted, vec![dir.join("mods/stale.jar")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn plan_sync_never_deletes_client_owned_files() {
        let dir = temp_game_dir("syncraft_test_plan_sync_protected");
        std::fs::write(dir.join("authlib-injector.jar"), b"jar").unwrap();
        std::fs::write(dir.join("options.txt"), b"options").unwrap();

        SyncState {
            version_hash: "old".into(),
            managed: [
                PathBuf::from("authlib-injector.jar"),
                PathBuf::from("options.txt"),
            ]
            .into_iter()
            .collect(),
            manifest: None,
        }
        .save(&dir.join(STATE_FILE))
        .unwrap();

        // An empty manifest means everything previously managed is gone.
        let manifest = manifest_with(vec![]);
        let plan = plan_sync(&manifest, &dir).await.unwrap();

        assert!(
            !plan.to_delete.contains(&dir.join("authlib-injector.jar")),
            "client-owned jar must be protected"
        );
        assert!(
            plan.to_delete.contains(&dir.join("options.txt")),
            "unprotected stale file should be deleted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
