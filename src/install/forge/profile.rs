use std::{collections::HashMap, fs, io::Read, path::{Path, PathBuf}, process::Command};

use crate::{Error, download::{meta::forge::{InstallProfileMeta, ProcessorMeta}, sha1_bytes}, install::{expand_args, replace_vars}, package::PackageName, paths::MinecraftPaths};

pub struct InstallProfile {
    // pub loader_version: String,
    // pub vanilla_version: String,
    // pub variables: HashMap<String, String>,
    pub processors: Vec<Processor>
}

impl InstallProfile {
    pub fn parse(meta: InstallProfileMeta, forge_dir: &PathBuf, paths: &MinecraftPaths) -> Result<Self, Error> {

        let vars = Self::build_vars(&meta, forge_dir, paths);

        let mut processors = Vec::new();

        for processor_meta in meta.processors {
            if processor_meta.sides.is_empty()
            || processor_meta.sides.contains(&"client".to_string()) {
                processors.push(Self::parse_processor(processor_meta, &vars, paths)?);
            }
        }

        Ok(Self {
            // loader_version: meta.version,
            // vanilla_version: meta.minecraft,
            // variables: vars,
            processors
        })
    }

    fn build_vars(meta: &InstallProfileMeta, forge_dir: &PathBuf, paths: &MinecraftPaths) -> HashMap<String, String> {
        let mut vars: HashMap<String, String> = HashMap::new();

        vars.insert("SIDE".into(), "client".into());
        vars.insert("MINECRAFT_VERSION".into(), meta.minecraft.clone());
        vars.insert("MINECRAFT_JAR".into(), paths.version_jar().to_string_lossy().into_owned());
        vars.insert("ROOT".into(), paths.root().to_string_lossy().into_owned());
        vars.insert("LIBRARY_DIR".into(), paths.libraries_dir().to_string_lossy().into_owned());

        for (key, entry) in &meta.data {

            if let Some(value) = entry.client.as_ref() {
                vars.insert(key.into(), Self::expand_maven_path(
                    &if value.starts_with("/") {
                        forge_dir.to_string_lossy().into_owned() + value
                    } else {
                        value.to_string()
                    },
                    paths
                ));
                
            }
        }

        vars
    }

    fn expand_maven_path(value: &str, paths: &MinecraftPaths) -> String {
        if value.starts_with("[")
        && value.ends_with("]")
        && let Ok(pkgname) = &value[1..value.len() - 1].parse::<PackageName>() {
            paths.library_path(&pkgname.to_maven_path()).to_string_lossy().into_owned()
        } else {
            value.to_string()
        }
    }

    fn parse_processor(
        meta: ProcessorMeta, 
        vars: &HashMap<String, String>,
        paths: &MinecraftPaths
    ) -> Result<Processor, Error> {
        
        let executable_jar = paths.library_path(
            &meta.jar.parse::<PackageName>()?.to_maven_path()
        );
        
        let mut classpath: Vec<PathBuf> = Vec::new();
        classpath.push(executable_jar.clone());
        for cp in meta.classpath {
            classpath.push(paths.library_path(
                &cp.parse::<PackageName>()?.to_maven_path()
            ));
        }

        let args = expand_args(&meta.args, &vars, |key| format!("{{{}}}", key))
            .iter().map(|value| Self::expand_maven_path(value, paths)).collect();

        let mut outputs = HashMap::new();
        for (key, value) in meta.outputs {
            outputs.insert(
                replace_vars(&key, &vars, |key| format!("{{{}}}", key)),
                replace_vars(&value, &vars, |key| format!("{{{}}}", key))
            );
        }

        Ok(Processor {
            executable_jar,
            classpath,
            args,
            outputs
        })
    }
}

#[derive(Debug)]
pub struct Processor {
    pub executable_jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    pub outputs: HashMap<String, String>
}

impl Processor {
    pub fn execute(&self, java_path: &PathBuf) -> Result<(), Error> {

        if self.verified() {
            return Ok(());
        }

        let classpath = self.build_classpath()?;
        let main_class = self.extract_main_class()?;

        let mut cmd = Command::new(java_path);

        cmd.arg("-cp");
        cmd.arg(classpath);
        cmd.arg(main_class);
        cmd.args(&self.args);

        let status = cmd.status()
            .map_err(|_| Error::Other("failed to start processor".to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Other(format!("processor failed with code {:?}", status.code())))
        }
    }

    fn build_classpath(&self) -> Result<std::ffi::OsString, Error> {
        std::env::join_paths(&self.classpath)
            .map_err(|err| Error::Other(err.to_string()))
    }

    fn extract_main_class(&self) -> Result<String, Error> {

        let file = fs::File::open(&self.executable_jar)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut manifest_file = archive.by_name("META-INF/MANIFEST.MF")?;

        let mut contents = String::new();
        manifest_file.read_to_string(&mut contents)?;

        for line in contents.lines() {
            if line.starts_with("Main-Class:") {
                let main_class = line.trim_start_matches("Main-Class:").trim();
                if main_class.is_empty() {
                    return Err(Error::Other("empty 'Main-Class' in MANIFEST.MF".to_string()));
                } else {
                    return Ok(main_class.to_string());
                }
            }
        }

        Err(Error::Other("'Main-Class' not found in MANIFEST.MF".to_string()))
    }

    fn verified(&self) -> bool {
        if self.outputs.is_empty() {
            false
        } else {
            !self.outputs.iter().any(|(path, hash)| {
                !Self::verify_file_hash(Path::new(path), hash).unwrap_or(false)
            })
        }
    }

    fn verify_file_hash(path: &Path, expected: &str) -> Result<bool, Error> {
        if path.exists() {
            let bytes = fs::read(path)?;
            let sha1 = sha1_bytes(&bytes);
            Ok(expected.eq_ignore_ascii_case(&sha1))
        } else {
            Ok(false)
        }
    }
}