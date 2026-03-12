# assfid_ffed

CubeSat image processing pipeline for the Jetson Orin Nano. Captures RAW images (ARW) via gphoto2, converts to TIFF with CUDA-accelerated debayering, and dispatches results through RabbitMQ.

## Architecture

- **Capture**: gphoto2 (camera) or mock (file-based, for testing)
- **Pipeline**: RAW → debayer (NPP/CUDA on Jetson, CPU fallback) → TIFF
- **Queue**: RabbitMQ — converted TIFFs are published to `tiff_queue`
- **Scheduling**: configurable interval (default 45s), optional start time or immediate trigger
- **Deploy target**: Jetson Orin Nano via Docker + Ansible

## Requirements

- Rust toolchain
- Docker + Docker Compose
- Ansible (`pip install ansible`)
- `~/.ffed_vault_pass` — vault password file for Ansible secrets

## Commands

```sh
cargo run                      # run with defaults (mock capture + mock queue)
cargo run -- config.mock.toml  # run with mock config explicitly
cargo run -- config.toml       # run with production config (needs RabbitMQ + camera)
cargo deploy                   # deploy to the Jetson via Ansible
cargo portainer                # start local Portainer UI and open browser
```

## Configuration

The app is configured via a TOML file passed as the first argument. See `config.mock.toml` for local testing and `config.toml` for production. Key options:

| Section | Key | Default | Description |
|---------|-----|---------|-------------|
| `capture` | `source` | `mock` | `mock` or `gphoto2` |
| `capture` | `interval_secs` | `45` | Seconds between captures |
| `capture` | `start_time` | none | `HH:MM:SS` to delay start |
| `capture` | `mock_file` | `input.arw` | File to use in mock mode |
| `queue` | `backend` | `mock` | `mock` or `rabbitmq` |
| `queue` | `rabbitmq_url` | — | AMQP connection string |
| `pipeline` | `debayer` | `true` | Enable debayering |
| `pipeline` | `compression` | `none` | `none`, `lzw`, or `deflate` |

## Deployment

### Prerequisites

- Connected to the same LAN as the Jetson (or via VPN)
- The Jetson resolves as `jetson.local` and is reachable via SSH as `sm` using `~/.ssh/assfid_ffed`. Get the private key from a team member and place it at `~/.ssh/assfid_ffed`:
  ```sh
  chmod 600 ~/.ssh/assfid_ffed
  ```
- `ansible/group_vars/all/vault.yml` — Ansible vault file containing secrets (`vault_become_password`, `vault_rabbitmq_password`, `ghcr_token`). Get this from a team member.
- `~/.ffed_vault_pass` — plaintext file containing the vault decryption password

```sh
echo 'your-vault-password' > ~/.ffed_vault_pass
chmod 600 ~/.ffed_vault_pass
```

`cargo deploy` runs the Ansible playbook which:
1. Installs Docker on the Jetson (if needed)
2. Deploys RabbitMQ, the app, and the Portainer agent via Docker Compose

To override the image tag:
```sh
ansible-playbook ansible/playbook.yml -e app_image_tag=v1.2.3
```

## Portainer

A Portainer agent runs on the Jetson. `cargo portainer` starts Portainer CE locally and connects to it, accessible at `http://portainer.localhost`.

First-time setup requires a `portainer/.env` file:
```sh
cp portainer/.env.example portainer/.env
# set PORTAINER_ADMIN_PASSWORD
```

## CI

Pushing to `main` (or tagging `v*`) builds and pushes an ARM64 image to GHCR and updates `deploy.env` with the new tag automatically.
