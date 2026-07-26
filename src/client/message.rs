use console_info::{format_reason_hint, msgs};
use owo_colors::OwoColorize;
use syncraft_protocol::{Loader, VersionInfo};

pub enum MessageKey<'a> {
    Entrance,
    ConfirmLink,
    InputLink,
    Connecting(&'a str),
    LinkInvalid,
    ConfirmJavaPath,
    SelectJava(Option<u8>),
    ConfirmAuth,
    SelectAuthMethod,
    OfflineMethod,
    SyncraftMethodLogin,
    SyncraftMethodSignUp,
    InputUsername,
    InvalidUsername,
    InputEmail,
    InputPassword,
    InputPasswordAgain,
    EmptyPassword,
    PasswordTooShort,
    InvalidPassword,
    PasswordUnmatch,
    VerifyMail,
    Syncing,
    SyncDone,
    Launching,
    TokenExpired,
    UpdateLog,
    SelectSkinType,
    SkinModelClassic,
    SkinModelSlim,
    UploadingSkin,
}

fn logo() -> &'static [&'static str] {
    &[
        "███████╗██╗   ██╗███╗   ██╗ ██████╗██████╗  █████╗ ███████╗████████╗",
        "██╔════╝╚██╗ ██╔╝████╗  ██║██╔════╝██╔══██╗██╔══██╗██╔════╝╚══██╔══╝",
        "███████╗ ╚████╔╝ ██╔██╗ ██║██║     ██████╔╝███████║█████╗     ██║   ",
        "╚════██║  ╚██╔╝  ██║╚██╗██║██║     ██╔══██╗██╔══██║██╔══╝     ██║   ",
        "███████║   ██║   ██║ ╚████║╚██████╗██║  ██║██║  ██║██║        ██║   ",
        "╚══════╝   ╚═╝   ╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝        ╚═╝   ",
        "====================================================================",
    ]
}

pub fn translate(key: &MessageKey) -> String {
    match key {
        MessageKey::Entrance => logo().join("\n"),
        MessageKey::ConfirmLink => msgs! {
            En: format!("Continue to use the link above? (y/n)"),
            Zh: format!("继续使用上面的链接？ (y/n)")
        },
        MessageKey::InputLink => msgs! {
            En: format!("Input your syncraft Link: "),
            Zh: format!("请输入 syncraft 链接： ")
        },
        MessageKey::Connecting(link) => msgs! {
            En: format!("Connecting to {link}..."),
            Zh: format!("连接 {link} ...")
        },
        MessageKey::LinkInvalid => {
            let reason = msgs! {
                En: "Cannot connect to syncraft link.",
                Zh: "无法连接到 syncraft 链接。"
            };
            let hint = msgs! {
                En: "Contact your server owner or try a different link.",
                Zh: "请联系您的腐竹或尝试重新输入链接。"
            };
            format_reason_hint(&reason, &hint)
        }
        MessageKey::ConfirmJavaPath => msgs! {
            En: format!("Continue to use the Java installation above? (y/n)"),
            Zh: format!("继续使用上面的 Java ？ (y/n)")
        },
        MessageKey::SelectJava(version) => {
            let hint = version
                .and_then(|v| Some(format!("\n( Recommend Version {} ) ", v)))
                .unwrap_or("".to_string());
            msgs! {
                En: format!("Select a Java on your device{}: ", hint),
                Zh: format!("选择您设备上的一个 Java {}：", hint)
            }
        }
        MessageKey::ConfirmAuth => msgs! {
            En: "Continue to use the account above? (y/n)",
            Zh: "继续使用上面的账号？ (y/n)"
        },
        MessageKey::SelectAuthMethod => msgs! {
            En: "Select your authentication method: ",
            Zh: "请选择您的认证方式："
        },
        MessageKey::OfflineMethod => msgs! {
            En: "Offline (You can join the syncraft server if its 'identity falsification' is on)",
            Zh: "离线模式（通过 syncraft 服务端可开启的“身份伪造”加入服务器）"
        },
        MessageKey::SyncraftMethodLogin => msgs! {
            En: "Syncraft Login (If you have an account; put a 'skin.png' in the directory to use it)",
            Zh: "Syncraft 登录（如果您已有账号；将命名为“skin.png”的皮肤文件放在此目录下以使用皮肤）"
        },
        MessageKey::SyncraftMethodSignUp => msgs! {
            En: "Syncraft Sign Up (Create an Syncraft account at this server)",
            Zh: "Syncraft Sign Up (在此服务器创建一个 Syncraft 账户)"
        },
        MessageKey::InputUsername => msgs! {
            En: "Please enter your username: ",
            Zh: "请输入您的用户名："
        },
        MessageKey::InvalidUsername => msgs! {
            En: "Invalid username. Please try another.",
            Zh: "此用户名不合法。"
        },
        MessageKey::InputEmail => msgs! {
            En: "Please enter your email: ",
            Zh: "请输入您的邮箱："
        },
        MessageKey::InputPassword => msgs! {
            En: "Please enter your password: ",
            Zh: "请输入密码："
        },
        MessageKey::InputPasswordAgain => msgs! {
            En: "Enter your password again: ",
            Zh: "请再次输入密码："
        },
        MessageKey::EmptyPassword => msgs! {
            En: "Password cannot be empty",
            Zh: "密码不能为空"
        },
        MessageKey::PasswordTooShort => msgs! {
            En: "Password must be at least 8 characters",
            Zh: "密码最少为 8 个字符"
        },
        MessageKey::InvalidPassword => msgs! {
            En: "Password contains invalid characters",
            Zh: "密码包含非法字符"
        },
        MessageKey::PasswordUnmatch => msgs! {
            En: "Password doesn't match. Please try again.",
            Zh: "密码不匹配，请重试。"
        },
        MessageKey::VerifyMail => msgs! {
            En: "A verify mail will be sent to your mailbox.\nPlease verify your email and login later.",
            Zh: "一封验证邮件已经发送到您的邮箱。\n请验证邮箱后登录。"
        },
        MessageKey::Syncing => msgs! {
            En: "Syncing game files...",
            Zh: "同步游戏文件..."
        },
        MessageKey::SyncDone => msgs! {
            En: "Sync complete.",
            Zh: "同步完成。"
        },
        MessageKey::Launching => msgs! {
            En: "Launching Minecraft...",
            Zh: "启动 Minecraft..."
        },
        MessageKey::TokenExpired => msgs! {
            En: "Session expired, please re-login.",
            Zh: "登录已过期，请重新登录。"
        },
        MessageKey::UpdateLog => msgs! {
            En: "Update Log:",
            Zh: "更新日志："
        },
        MessageKey::SelectSkinType => msgs! {
            En: "Select your skin model type: ",
            Zh: "请选择你的皮肤模型类型："
        },
        MessageKey::SkinModelClassic => msgs! {
            En: "Classic (Steve)",
            Zh: "经典模型 (Steve)"
        },
        MessageKey::SkinModelSlim => msgs! {
            En: "Slim (Alex)",
            Zh: "纤细模型 (Alex)"
        },
        MessageKey::UploadingSkin => msgs! {
            En: "Uploading skin...",
            Zh: "上传皮肤..."
        },
    }
}

/// Format a `VersionInfo` into a human-readable bullet list.
pub fn format_version_info(info: &VersionInfo) -> String {
    let name_label = msgs! { En: "Name", Zh: "名称" };
    let version_label = msgs! { En: "Minecraft", Zh: "游戏版本" };
    let loader_label = msgs! { En: "Loader", Zh: "加载器" };

    let loader_str = match &info.loader {
        Loader::Vanilla => msgs! {
            En: "Vanilla (No ModLoader)",
            Zh: "Vanilla (无 ModLoader)"
        },
        Loader::Fabric { loader_version } => {
            format!("Fabric {}", loader_version)
        }
        Loader::Forge { loader_version } => {
            format!("Forge {}", loader_version)
        }
        Loader::NeoForge { loader_version } => {
            format!("NeoForge {}", loader_version)
        }
    };

    vec![
        format!("  {}: {}", name_label.italic(), info.name.bold()),
        format!(
            "  {}: {}",
            version_label.italic(),
            info.vanilla_version.bold()
        ),
        format!("  {}: {}", loader_label.italic(), loader_str.bold()),
    ]
    .join("\n")
}

#[test]
fn test() {
    use crate::client::message::{MessageKey, translate};
    println!("{}", translate(&MessageKey::Entrance));
}
