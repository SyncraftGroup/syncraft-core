use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use crate::{
    Error,
    download::{ResourceKind, Sources, fetch},
    install::expand_args,
    paths::MinecraftPaths,
    version::Version,
};

const JVM_MODULE_OPENS: &[&str] = &[
    "--add-opens",
    "java.base/java.lang.invoke=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.lang.reflect=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.io=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.lang=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.util=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.nio=ALL-UNNAMED",
    "--add-opens",
    "java.base/sun.security.util=ALL-UNNAMED",
];

pub struct LaunchContext {
    pub java_path: PathBuf,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub launcher_name: String,
    pub launcher_version: String,
    pub auth_server: Option<String>,
}

pub struct Launcher {}

impl Launcher {
    pub async fn launch(
        version: Version,
        ctx: LaunchContext,
        paths: &MinecraftPaths,
        output_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Result<Child, Error> {
        let vars = Self::build_vars(&version, &ctx, paths)?;

        let mut jvm_args = expand_args(&version.jvm_args, &vars, |key| format!("${{{}}}", key));
        let game_args = expand_args(&version.game_args, &vars, |key| format!("${{{}}}", key));

        let module_path_jars = Self::extract_module_paths(&jvm_args);

        let classpath = Self::build_classpath(&version, &module_path_jars)?;

        jvm_args = Self::inject_classpath(jvm_args, &classpath);

        let main_class = version
            .main_class
            .as_ref()
            .ok_or(Error::Other("missing main class".to_string()))?;

        let mut cmd = Command::new(&ctx.java_path);

        cmd.args(JVM_MODULE_OPENS);

        if let Some(ref auth_server) = ctx.auth_server {
            let injector_path = paths.root().join("authlib-injector.jar");
            Self::ensure_authlib_injector(&injector_path).await?;
            cmd.arg(format!("-javaagent:{}", injector_path.display()));
            cmd.arg(format!(
                "-javaDauthlibinjector.yggdrasilServer={}",
                auth_server
            ));
        }

        cmd.args(&jvm_args);
        cmd.arg(main_class);
        cmd.args(&game_args);
        cmd.current_dir(&paths.root());

        if output_tx.is_some() {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        }

        let mut child = cmd.spawn()?;

        if let Some(tx) = output_tx {
            if let Some(stdout) = child.stdout.take() {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        if tx.blocking_send(line).is_err() {
                            break;
                        }
                    }
                });
            }
            if let Some(stderr) = child.stderr.take() {
                std::thread::spawn(move || {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().flatten() {
                        if tx.blocking_send(line).is_err() {
                            break;
                        }
                    }
                });
            }
        }

        Ok(child)
    }

    fn extract_module_paths(jvm_args: &[String]) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();
        let mut i = 0;
        while i < jvm_args.len() {
            match jvm_args[i].as_str() {
                "--module-path" | "-p" => {
                    if i + 1 < jvm_args.len() {
                        for p in jvm_args[i + 1].split(':') {
                            paths.insert(PathBuf::from(p));
                        }
                    }
                }
                arg if arg.starts_with("--module-path=") => {
                    for p in arg.trim_start_matches("--module-path=").split(':') {
                        paths.insert(PathBuf::from(p));
                    }
                }
                arg if arg.starts_with("-p=") => {
                    for p in arg.trim_start_matches("-p=").split(':') {
                        paths.insert(PathBuf::from(p));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        paths
    }

    fn inject_classpath(mut jvm_args: Vec<String>, classpath_str: &std::ffi::OsStr) -> Vec<String> {
        let cp = classpath_str.to_string_lossy();
        for arg in &mut jvm_args {
            if arg.starts_with("-cp=") {
                *arg = format!("-cp={cp}");
            } else if arg.starts_with("--class-path=") {
                *arg = format!("--class-path={cp}");
            }
        }
        // Replace the value after standalone -cp/--class-path
        let mut i = 0;
        while i < jvm_args.len() {
            if jvm_args[i] == "-cp" || jvm_args[i] == "--class-path" {
                if i + 1 < jvm_args.len() {
                    jvm_args[i + 1] = cp.to_string();
                }
            }
            i += 1;
        }
        jvm_args
    }

    fn build_vars(
        version: &Version,
        ctx: &LaunchContext,
        paths: &MinecraftPaths,
    ) -> Result<HashMap<String, String>, Error> {
        let mut vars: HashMap<String, String> = HashMap::new();

        vars.insert("classpath".into(), String::new());
        vars.insert(
            "natives_directory".into(),
            paths.natives_dir().to_string_lossy().into_owned(),
        );
        vars.insert(
            "game_directory".into(),
            paths.root().to_string_lossy().into_owned(),
        );
        vars.insert(
            "library_directory".into(),
            paths.libraries_dir().to_string_lossy().into_owned(),
        );
        vars.insert("version_name".into(), paths.version_name.clone());
        vars.insert(
            "classpath_separator".into(),
            Self::classpath_separator().into(),
        );
        vars.insert("auth_player_name".into(), ctx.username.clone());
        vars.insert("auth_uuid".into(), ctx.uuid.clone());
        vars.insert("auth_access_token".into(), ctx.access_token.clone());
        vars.insert("auth_xuid".into(), String::new());
        vars.insert("clientid".into(), String::new());
        vars.insert("user_type".into(), "msa".into());
        vars.insert("version_type".into(), "release".into());
        vars.insert("launcher_name".into(), ctx.launcher_name.clone());
        vars.insert("launcher_version".into(), ctx.launcher_version.clone());
        vars.insert(
            "assets_root".into(),
            paths.assets_dir().to_string_lossy().into_owned(),
        );
        vars.insert("assets_index_name".into(), version.assets.id.clone());
        vars.insert("quickPlayMultiplayer".into(), String::new());
        vars.insert("quickPlayPath".into(), String::new());
        vars.insert("quickPlayRealms".into(), String::new());
        vars.insert("quickPlaySingleplayer".into(), String::new());
        vars.insert("resolution_width".into(), "854".into());
        vars.insert("resolution_height".into(), "480".into());

        Ok(vars)
    }

    fn classpath_separator() -> &'static str {
        if cfg!(windows) { ";" } else { ":" }
    }

    fn build_classpath(
        version: &Version,
        module_path_jars: &HashSet<PathBuf>,
    ) -> Result<std::ffi::OsString, Error> {
        let mut paths = Vec::new();
        for lib in version.libraries.values() {
            if module_path_jars.contains(&lib.file.path)
                || Self::conflicts_with_module_path(&lib.name, module_path_jars)
            {
                continue;
            }
            paths.push(lib.file.path.clone());
        }
        let client = version
            .version_jar
            .as_ref()
            .ok_or(Error::Other("missing client jar".to_string()))?;
        if !module_path_jars.contains(&client.path) {
            paths.push(client.path.clone());
        }
        std::env::join_paths(paths).map_err(|err| Error::Other(err.to_string()))
    }

    /// Check whether a library already has a conflicting jar on the module path.
    /// Two jars conflict when they share the same Maven artifact (groupId:artifactId)
    /// but differ in version — the module-path version takes precedence.
    fn conflicts_with_module_path(
        name: &crate::package::PackageName,
        module_jars: &HashSet<PathBuf>,
    ) -> bool {
        let maven_prefix = format!("{}/{}/", name.group.replace('.', "/"), name.artifact);
        module_jars
            .iter()
            .any(|mj| mj.to_string_lossy().contains(&maven_prefix))
    }

    async fn ensure_authlib_injector(path: &PathBuf) -> Result<(), Error> {
        if path.exists() {
            return Ok(());
        }
        let source = Sources::selected();
        let info_urls = Sources::urls(ResourceKind::AuthlibInjectorMeta, &source)?;
        let resp = fetch(&info_urls).await?;
        let info: AuthlibInjectorInfo = resp
            .json()
            .await
            .map_err(|e| Error::Other(format!("authlib-injector metadata: {e}")))?;
        let jar_urls = vec![info.download_url];
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = fetch(&jar_urls)
            .await?
            .bytes()
            .await
            .map_err(|e| Error::Other(format!("authlib-injector download: {e}")))?;
        tokio::fs::write(path, &bytes).await?;
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct AuthlibInjectorInfo {
    download_url: String,
}
