use std::{collections::HashMap, path::PathBuf, process::{Child, Command}};

use crate::{Error, install::expand_args, paths::MinecraftPaths, version::Version};

pub struct LaunchContext {
    pub java_path: PathBuf,
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub launcher_name: String,
    pub launcher_version: String,
}

pub struct Launcher { }

impl Launcher {
    pub fn launch(version: Version, ctx: LaunchContext, paths: &MinecraftPaths) -> Result<Child, Error> {
        
        let vars = Self::build_vars(&version, &ctx, paths)?;
        
        let jvm_args = expand_args(&version.jvm_args, &vars, |key| format!("${{{}}}", key));
        let game_args = expand_args(&version.game_args, &vars, |key| format!("${{{}}}", key));

        // println!("{:?}", jvm_args);
        // println!("{:?}", game_args);
        
        let main_class = version.main_class.as_ref()
            .ok_or(Error::Other("missing main class".to_string()))?;

        let mut cmd = Command::new(&ctx.java_path);

        cmd.args(&jvm_args);
        cmd.arg(main_class);
        cmd.args(&game_args);
        cmd.current_dir(&paths.root()); 

        Ok(cmd.spawn()?)
    }

    fn build_vars(
        version: &Version,
        ctx: &LaunchContext,
        paths: &MinecraftPaths
    ) -> Result<HashMap<String, String>, Error> {
        
        let mut vars: HashMap<String, String> = HashMap::new();

        vars.insert("classpath".into(), Self::build_classpath(version)?.to_string_lossy().into_owned());
        vars.insert("natives_directory".into(), paths.natives_dir().to_string_lossy().into_owned());
        vars.insert("game_directory".into(), paths.root().to_string_lossy().into_owned());
        vars.insert("auth_player_name".into(), ctx.username.clone());
        vars.insert("auth_uuid".into(), ctx.uuid.clone());
        vars.insert("auth_access_token".into(), ctx.access_token.clone());
        vars.insert("launcher_name".into(), ctx.launcher_name.clone());
        vars.insert("launcher_version".into(), ctx.launcher_version.clone());
        vars.insert("assets_root".into(), paths.assets_dir().to_string_lossy().into_owned());
        vars.insert("assets_index_name".into(), version.assets.id.clone());

        Ok(vars)
    }

    fn build_classpath(version: &Version) -> Result<std::ffi::OsString, Error> {
        let mut paths = Vec::new();
        for lib in version.libraries.values() {
            paths.push(lib.file.path.clone());
        }
        let client = version.version_jar.as_ref()
            .ok_or(Error::Other("missing client jar".to_string()))?;
        paths.push(client.path.clone());
        std::env::join_paths(paths)
            .map_err(|err| Error::Other(err.to_string()))
    }
}