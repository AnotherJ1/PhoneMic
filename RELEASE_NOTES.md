# PhoneMic v__VERSION__

Released: __DATE__

This template is filled in by `.github/workflows/release.yml` (task 14.3)
and `scripts/generate-release-notes.sh` (task 14.5).

> The generator groups commits since the previous tag by
> [conventional-commits](https://www.conventionalcommits.org/) prefix:
> `feat`, `fix`, `docs`, `refactor`, `test`, `chore`. Commits without
> a recognised prefix go into "Other changes".

### Features

<!-- populated from `git log` feat: entries -->

### Bug fixes

<!-- populated from `git log` fix: entries -->

### Documentation

<!-- populated from `git log` docs: entries -->

### Refactoring

<!-- populated from `git log` refactor: entries -->

### Tests

<!-- populated from `git log` test: entries -->

### Chores

<!-- populated from `git log` chore: entries -->

### Other changes

<!-- catch-all -->

---

## Verification checklist

- [ ] `cargo test --workspace --all-features` PASS
- [ ] `pnpm -C apps/mobile test` PASS (all 34 fast-check properties)
- [ ] `pnpm -C apps/mobile e2e` PASS
- [ ] `scripts/smoke.{ps1,sh}` PASS on Windows / macOS / Linux runners
- [ ] Artefacts signed where secrets are configured (see release.yml summary)

## Permissions notes (for end-users)

- macOS Accessibility prompt is required for keyboard injection.
- Phone browser microphone permission is required for voice input.

## Known limitations

- Linux pure Wayland: keyboard injection requires XWayland.
- Server_ASR (whisper.cpp) ships with `small` / `base` models; first run
  copies them to the user data dir.
