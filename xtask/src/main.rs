use std::{
    env,
    process::{Command, exit},
};

fn main() {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("portainer") => portainer(),
        Some("deploy") => deploy(),
        _ => {
            eprintln!("Usage: cargo xtask <task>");
            eprintln!("Tasks:");
            eprintln!("  portainer  Start Portainer and open browser");
            eprintln!("  deploy     Run the Ansible playbook");
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
