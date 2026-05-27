```bash
cargo run -- \
  --source-host <SOURCE_HOST> \
  --source-username <SOURCE_USERNAME> \
  --source-password <SOURCE_PASSWORD> \
  --source-port <SOURCE_PORT> \
  --source-database <SOURCE_DATABASE> \
  [--source-requires-tls] \
  --target-host <TARGET_HOST> \
  --target-username <TARGET_USERNAME> \
  --target-password <TARGET_PASSWORD> \
  --target-port <TARGET_PORT> \
  --target-database <TARGET_DATABASE> \
  [--target-requires-tls]
```

## Linux Requirements

If your database provider requires SSL/TLS connections, OpenSSL must be installed.

### Ubuntu / Debian

```bash
sudo apt update
sudo apt install openssl libssl-dev ca-certificates
```

### Fedora / RHEL

```bash
sudo dnf install openssl openssl-devel ca-certificates
```

### Arch Linux

```bash
sudo pacman -S openssl ca-certificates
```
