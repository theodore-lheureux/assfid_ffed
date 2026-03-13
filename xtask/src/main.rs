use std::{
    env,
    path::Path,
    process::{Command, exit},
};

const LOCAL_IMAGE_NAME: &str = "assfid_ffed:local";
const LOCAL_IMAGE_ARCHIVE: &str = "/tmp/assfid_ffed-jetson-image.tar";
const LOCAL_IMAGE_PLATFORM: &str = "linux/arm64";

fn main() {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("portainer") => portainer(),
        Some("deploy") => deploy(),
        Some("deploy-local") => deploy_local(),
        _ => {
            eprintln!("Usage: cargo xtask <task>");
            eprintln!("Tasks:");
            eprintln!("  portainer  Start Portainer and open browser");
            eprintln!("  deploy     Run the Ansible playbook");
            eprintln!("  deploy-local  Build the image locally and deploy with Ansible");
            exit(1);
        }
    }
}

fn portainer() {
    let root = project_root();

    let compose = root.join("portainer").join("compose.yml");

    run(Command::new("docker")
        .args(["compose", "-f", compose.to_str().unwrap(), "up", "-d"]));

    let url = "http://portainer.localhost";

    #[cfg(target_os = "macos")]
    run(Command::new("open").arg(url));

    #[cfg(target_os = "windows")]
    run(Command::new("cmd").args(["/c", "start", url]));

    #[cfg(target_os = "linux")]
    run(Command::new("xdg-open").arg(url));
}

fn deploy() {
    let root = project_root();
    run(Command::new("ansible-playbook")
        .arg(root.join("ansible").join("playbook.yml")));
}

fn deploy_local() {
    let root = project_root();

    run(Command::new("docker")
        .arg("version"));

    run(Command::new("docker")
        .args(["buildx", "version"]));

    run(Command::new("docker")
        .current_dir(&root)
        .args([
            "buildx",
            "build",
            "--platform",
            LOCAL_IMAGE_PLATFORM,
            "--progress",
            "plain",
            "--load",
            "-f",
            "Dockerfile.jetson",
            "-t",
            LOCAL_IMAGE_NAME,
            ".",
        ]));

    if let Some(parent) = Path::new(LOCAL_IMAGE_ARCHIVE).parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("Failed to create {:?}: {}", parent, e);
            exit(1);
        });
    }

    run(Command::new("docker")
        .args(["save", "-o", LOCAL_IMAGE_ARCHIVE, LOCAL_IMAGE_NAME]));

    run(Command::new("ansible-playbook")
        .arg(root.join("ansible").join("playbook.yml"))
        .args([
            "-e",
            "app_build_local=true",
            "-e",
            "app_skip_local_build=true",
        ]));
}

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("Failed to run {:?}: {}", cmd.get_program(), e);
        exit(1);
    });
    if !status.success() {
        exit(status.code().unwrap_or(1));
    }
}

fn project_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is xtask/, so go up one level
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
