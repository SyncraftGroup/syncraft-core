//! 解析包名称，用于从 Maven 下载库

use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use crate::{Error, version::LibKey};

#[derive(Debug, Clone, Default)]
pub struct PackageName {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl PackageName {
    pub fn to_maven_path(&self) -> String {
        let group = self.group.replace('.', "/");
        let mut file_name = format!("{}-{}", self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            file_name.push('-');
            file_name.push_str(classifier);
        }
        file_name.push('.');
        file_name.push_str(&self.extension);

        format!("{}/{}/{}/{}", group, self.artifact, self.version, file_name)
    }

    pub fn key(&self) -> LibKey {
        LibKey {
            group: self.group.clone(),
            artifact: self.artifact.clone(),
            classifier: self.classifier.clone(),
        }
    }
}

impl FromStr for PackageName {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // split on extension name
        // e.g. natives-windows@zip
        let (main, extension) = if let Some((a, b)) = s.split_once('@') {
            (a, b.to_string())
        } else {
            (s, "jar".to_string())
        };

        let parts: Vec<&str> = main.split(':').collect();
        match parts.len() {
            // group:artifact:version
            3 => Ok(Self {
                group: parts[0].to_string(),
                artifact: parts[1].to_string(),
                version: parts[2].to_string(),
                classifier: None,
                extension,
            }),
            // group:artifact:version:classifier
            4 => Ok(Self {
                group: parts[0].to_string(),
                artifact: parts[1].to_string(),
                version: parts[2].to_string(),
                classifier: Some(parts[3].to_string()),
                extension,
            }),
            _ => Err(Error::Parse(format!("invalid package name: {s}"))),
        }
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut name = format!("{}:{}:{}", self.group, self.artifact, self.version);
        if let Some(classifier) = &self.classifier {
            name.push(':');
            name.push_str(classifier);
        }
        // ignore extensions
        write!(f, "{}", name)
    }
}

#[test]
fn parse_uri_test() {
    fn test_package(input: &str) {
        let result = PackageName::from_str(input);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(input, &format!("{result}"));
    }
    test_package("net.fabricmc:sponge-mixin:0.9.2+mixin.0.8.2");
    test_package("net.fabricmc:tiny-remapper:0.3.0.70");
    test_package("net.fabricmc:access-widener:1.0.0");
    test_package("net.fabricmc:fabric-loader-sat4j:2.3.5.4");
    test_package("com.google.jimfs:jimfs:1.2-fabric");
    test_package("org.ow2.asm:asm:9.1");
    test_package("org.ow2.asm:asm-analysis:9.1");
    test_package("org.ow2.asm:asm-commons:9.1");
    test_package("org.ow2.asm:asm-tree:9.1");
    test_package("org.ow2.asm:asm-util:9.1");
    test_package("net.fabricmc:intermediary:1.16.5");
    test_package("net.fabricmc:tiny-mappings-parser:0.2.2.14");
    test_package("net.fabricmc:fabric-loader:0.11.3");
}
