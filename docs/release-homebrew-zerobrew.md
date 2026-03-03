# Release Flow (Homebrew + ZeroBrew)

This checklist is for publishing a new `rliamp` release and ensuring both Homebrew and ZeroBrew install the same binary version.

## 1) Prepare and tag release

```bash
cargo fmt
cargo check
git status
git tag -a vX.Y.Z -m "rliamp vX.Y.Z"
git push origin main --tags
```

## 2) Publish source archive and checksum

After creating the GitHub release, compute the tarball checksum:

```bash
curl -L -o /tmp/rliamp-vX.Y.Z-src.tar.gz \
  https://github.com/0smboy/rliamp/releases/download/vX.Y.Z/rliamp-vX.Y.Z-src.tar.gz
shasum -a 256 /tmp/rliamp-vX.Y.Z-src.tar.gz
```

## 3) Update formula

Edit `Formula/rliamp.rb`:

- `url` -> `.../vX.Y.Z/rliamp-vX.Y.Z-src.tar.gz`
- `sha256` -> checksum from step 2

Then commit and push:

```bash
git add Formula/rliamp.rb
git commit -m "chore(release): update formula for vX.Y.Z"
git push origin main
```

## 4) Verify Homebrew install

```bash
brew update
brew reinstall 0smboy/rliamp/rliamp
rliamp --version
```

Expected output should contain `rliamp X.Y.Z`.

## 5) Verify ZeroBrew install

```bash
zb update
zb reinstall 0smboy/rliamp/rliamp
/opt/zerobrew/bin/rliamp --version
```

Expected output should contain `rliamp X.Y.Z`.

## 6) Binary parity check (optional but recommended)

```bash
cmp ./target-user/release/rliamp /opt/zerobrew/bin/rliamp
```

If `cmp` reports differences:

1. Confirm `Formula/rliamp.rb` points to the latest release tarball.
2. Confirm the release tarball was re-uploaded after tag creation (if re-uploaded, checksum must be refreshed).
3. Run `zb update` and reinstall again to bypass stale formula cache.
