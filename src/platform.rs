#[cfg(target_os = "windows")]
pub const OS_NAME: &str = "windows";
#[cfg(target_os = "macos")]
pub const OS_NAME: &str = "osx";
#[cfg(target_os = "linux")]
pub const OS_NAME: &str = "linux";

#[cfg(target_arch = "x86")]
pub const ARCH_NAME: &[&str] = &["x86", "32"];
#[cfg(target_arch = "x86_64")]
pub const ARCH_NAME: &[&str] = &["x86_64", "64"];
#[cfg(target_arch = "arm")]
pub const ARCH_NAME: &[&str] = &["arm", "64"];
#[cfg(target_arch = "aarch64")]
pub const ARCH_NAME: &[&str] = &["aarch64", "64"];

pub fn current_native_classifier() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "natives-windows"
    }

    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    {
        "natives-windows-x86"
    }

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "natives-windows-arm64"
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "natives-linux"
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "natives-linux-arm64"
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "natives-macos"
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "natives-macos-arm64"
    }
}


#[cfg(target_os="windows")]
pub const JAVA_ROOTS: &[&str] = &[
    r"C:\Program Files",
    r"C:\Program Files (x86)",
    r"D:\Program Files",
    r"D:\Program Files (x86)",
];

#[cfg(target_os="linux")]
pub const JAVA_ROOTS: &[&str] = &[
    "/usr/lib/jvm",
    "/usr/java",
    "/opt",
    "/opt/java",
    "/usr/local",
    "/usr/local/java",
    "/snap"
];

#[cfg(target_os="macos")]
pub const JAVA_ROOTS: &[&str] = &[
    "/Library/Java/JavaVirtualMachines",
    "/System/Library/Java/JavaVirtualMachines",
    "/opt/homebrew/opt",
    "/usr/local/opt"
];