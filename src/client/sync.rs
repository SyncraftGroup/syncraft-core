use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use futures::stream::StreamExt;

use syncraft_core::Error;
use syncraft_core::download::UiProgress;
use syncraft_protocol::{Manifest, SyncMode, UpdateInfo};

pub async fn fetch_manifest(base: &str) -> Result<Manifest, Error> {
    let url = format!("{base}/manifest");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(e))?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "failed to fetch manifest: {}",
            resp.status()
        )));
    }
    resp.json().await.map_err(|e| Error::Other(e.to_string()))
}

pub async fn fetch_update_info(base: &str, since_hash: &str) -> Result<Vec<UpdateInfo>, Error> {
    let url = format!("{base}/updateInfo/{since_hash}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(e))?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "failed to fetch update info: {}",
            resp.status()
        )));
    }
    resp.json().await.map_err(|e| Error::Other(e.to_string()))
}

pub struct SyncPlan {
    pub to_download: HashMap<blake3::Hash, Vec<DownloadAction>>,
    pub to_delete: Vec<PathBuf>,
}

pub struct DownloadAction {
    pub rel_path: PathBuf,
    pub dest: PathBuf,
}

pub async fn plan_sync(manifest: &Manifest, game_dir: &Path) -> Result<SyncPlan, Error> {
    let mut to_download: HashMap<blake3::Hash, Vec<DownloadAction>> = HashMap::new();
    let mut to_delete = Vec::new();

    let existing_paths = walk_game_dir(game_dir).await?;

    for (rel_path, sync_file) in &manifest.files {
        let dest = game_dir.join(rel_path);
        match sync_file.mode {
            SyncMode::Forbidden => {
                if dest.exists() {
                    to_delete.push(dest);
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
            SyncMode::Default => {
                if let Some(expected_state) = &sync_file.state {
                    if !dest.exists() {
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

    for existing_path in &existing_paths {
        let rel = trim_path(existing_path);
        if manifest_paths.contains(&rel) {
            continue;
        }
        if let Some(first) = rel.iter().next().and_then(|c| c.to_str()) {
            if [".minecraft", "syncraft_client.json", "config.json"].contains(&first) {
                continue;
            }
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
            if strict_dirs.contains(&dir.to_path_buf()) {
                should_delete = true;
                break;
            }
            ancestor = dir.parent();
        }
        if should_delete {
            to_delete.push(existing_path.clone());
        }
    }

    Ok(SyncPlan {
        to_download,
        to_delete,
    })
}

pub async fn download_files(
    base: &str,
    plan: &SyncPlan,
    progress: &UiProgress,
) -> Result<(), Error> {
    let hash_count = plan.to_download.len() as u64;
    progress.set_total(hash_count);

    let client = reqwest::Client::new();

    for (expected_hash, actions) in &plan.to_download {
        let hash_hex = expected_hash.to_hex();
        let url = format!("{base}/download/{hash_hex}");

        let name = actions
            .first()
            .and_then(|a| a.rel_path.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let pb = progress.add_download_bar(name, 0);

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Network(e))?;
        let total_size = response.content_length().unwrap_or(0);
        pb.set_length(total_size);

        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        let mut hasher = blake3::Hasher::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Error::Network(e))?;
            hasher.update(&chunk);
            bytes.extend_from_slice(&chunk);
            pb.inc(chunk.len() as u64);
        }

        let actual_hash = hasher.finalize();
        if actual_hash != *expected_hash {
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
            tokio::fs::write(&action.dest, &bytes).await?;
        }

        pb.finish_and_clear();
        progress.overall_inc();
    }

    for path in &plan.to_delete {
        if path.exists() {
            if path.is_dir() {
                tokio::fs::remove_dir_all(path).await?;
            } else {
                tokio::fs::remove_file(path).await?;
            }
        }
    }

    Ok(())
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
