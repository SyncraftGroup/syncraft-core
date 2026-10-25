//! Network tests for the BMCLAPI mirror path.
//!
//! Run with:
//!
//! ```text
//! SYNCRAFT_NET_TESTS=1 cargo test -p syncraft-core --test bmclapi -- --nocapture --test-threads=1
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use syncraft_core::Source;
use syncraft_core::download::{
    ResolvedFile, ResourceKind, Sources,
    downloader::Downloader,
    fetch,
    meta::{AssetIndex, VersionMeta, vanilla},
};

fn enabled() -> bool {
    if std::env::var_os("SYNCRAFT_NET_TESTS").is_none() {
        eprintln!("skipped: set SYNCRAFT_NET_TESTS=1 to run network tests");
        return false;
    }
    true
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Walks the metadata chain (manifest → version JSON → asset index) through the
/// BMCLAPI mirrors, which rewrite the piston-meta URLs.
#[tokio::test]
async fn bmclapi_manifest_version_meta_and_asset_index() {
    if !enabled() {
        return;
    }

    let urls = Sources::urls(ResourceKind::VanillaManifest, &Source::BMCLAPI).unwrap();
    println!("[manifest]    {}", urls[0]);
    let manifest: vanilla::Manifest = fetch(&urls).await.unwrap().json().await.unwrap();
    println!("[manifest]    {} versions", manifest.versions.len());
    assert!(manifest.versions.len() > 100);

    let tag = manifest.get_version_tag("1.21.1").unwrap();
    let urls = Sources::urls(ResourceKind::VanillaMeta(tag.clone()), &Source::BMCLAPI).unwrap();
    println!("[version]     {}", urls[0]);
    let meta: VersionMeta = fetch(&urls).await.unwrap().json().await.unwrap();
    assert!(meta.main_class.is_some());

    let asset_index = meta.asset_index.expect("1.21.1 has an asset index");
    let urls = Sources::urls(ResourceKind::AssetIndex(asset_index), &Source::BMCLAPI).unwrap();
    println!("[asset index] {}", urls[0]);
    let index: AssetIndex = fetch(&urls).await.unwrap().json().await.unwrap();
    println!("[asset index] {} objects", index.objects.len());
    assert!(index.objects.len() > 100);
}

/// Downloads more files than BMCLAPI's short-window rate limit (10) in
/// parallel. Without the per-source concurrency cap this floods the mirror with
/// 429s; relying on retries alone makes it very slow.
#[tokio::test]
async fn bmclapi_parallel_library_downloads() {
    if !enabled() {
        return;
    }

    const LIBRARIES: &[&str] = &[
        "com/google/code/gson/gson/2.10.1/gson-2.10.1.jar",
        "org/slf4j/slf4j-api/2.0.9/slf4j-api-2.0.9.jar",
        "org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0.jar",
        "org/apache/logging/log4j/log4j-api/2.22.1/log4j-api-2.22.1.jar",
        "com/mojang/brigadier/1.1.8/brigadier-1.1.8.jar",
        "com/mojang/authlib/6.0.54/authlib-6.0.54.jar",
        "com/mojang/datafixerupper/8.0.16/datafixerupper-8.0.16.jar",
        "com/google/code/findbugs/jsr305/3.0.2/jsr305-3.0.2.jar",
        "org/joml/joml/1.10.5/joml-1.10.5.jar",
        "net/sf/jopt-simple/jopt-simple/5.0.4/jopt-simple-5.0.4.jar",
        "io/netty/netty-common/4.1.97.Final/netty-common-4.1.97.Final.jar",
        "com/google/guava/guava/32.1.2-jre/guava-32.1.2-jre.jar",
        "org/apache/logging/log4j/log4j-core/2.22.1/log4j-core-2.22.1.jar",
    ];

    let concurrency = Sources::max_concurrency(&Source::BMCLAPI);
    println!(
        "[download]    concurrency={concurrency}, {} files (mirror limit is ~10 in flight)",
        LIBRARIES.len()
    );

    let dir = temp_dir("syncraft_test_bmclapi_download");
    let files: Vec<ResolvedFile> = LIBRARIES
        .iter()
        .map(|path| {
            let file_name = path.rsplit('/').next().unwrap_or("file").to_string();
            ResolvedFile {
                url: Some(
                    Sources::urls(
                        ResourceKind::Library {
                            path: (*path).to_string(),
                            repo_url: None,
                        },
                        &Source::BMCLAPI,
                    )
                    .unwrap(),
                ),
                path: dir.join(file_name),
                sha1: None,
                size: None,
            }
        })
        .collect();

    let downloader = Downloader::new(concurrency, Duration::from_secs(600), 5);
    let start = Instant::now();
    downloader
        .ensure_all(files.iter(), None)
        .await
        .expect("all files should download from BMCLAPI");
    println!(
        "[download]    finished in {:.1}s",
        start.elapsed().as_secs_f32()
    );

    for file in &files {
        let size = std::fs::metadata(&file.path)
            .unwrap_or_else(|e| panic!("{}: {e}", file.path.display()))
            .len();
        println!(
            "[download]    {:<40} {:>9} bytes",
            file.path.file_name().unwrap().to_string_lossy(),
            size
        );
        assert!(size > 0, "empty file: {}", file.path.display());
    }

    let _ = std::fs::remove_dir_all(&dir);
}
