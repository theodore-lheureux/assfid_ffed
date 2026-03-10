# assfid_ffed

RAW image processing pipeline for the CubeSat ground station. Runs on a Jetson Orin Nano, converting camera RAW files (ARW) to TIFF using CUDA-accelerated debayering via NPP.

## Architecture

- **App**: Rust binary, built for `aarch64` with CUDA/NPP acceleration
- **Message queue**: RabbitMQ (job dispatch)
- **Deploy target**: Jetson Orin Nano via Docker + Ansible

## Requirements

- Rust toolchain
- Docker + Docker Compose
- Ansible (`pip install ansible`)
- `~/.ffed_vault_pass` — vault password file for Ansible secrets

## Commands

```sh
cargo deploy      # deploy to the Jetson via Ansible
cargo portainer   # start local Portainer UI and open browser
```

## Deployment

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
