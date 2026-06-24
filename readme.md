This is a easy-to-use cli tool for cloning entire postgres schemas from a source database to a target database. It can also copy table data from a source database. This tool will clone:
- tables
- columns  
- views
- primary keys
- foreign keys
- extensions (you will need to download manually on source database before running command)

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
---

If your database provider requires SSL/TLS connections, OpenSSL must be installed.
