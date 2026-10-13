# Blitz fork workflow

`apps/native` runs on [Blitz] via [`dioxus-native`]. We pull both from our
own fork at <https://github.com/FastTrackStudios/blitz>, branch
`fts/integration`, instead of crates.io or upstream `main` directly. This
lets us land bug-fixes locally **and** contribute them upstream cleanly.

[Blitz]: https://github.com/DioxusLabs/blitz
[`dioxus-native`]: https://github.com/DioxusLabs/blitz/tree/main/packages/dioxus-native

## Branch layout in `FastTrackStudios/blitz`

| Branch                                     | Purpose                                                                 |
|--------------------------------------------|-------------------------------------------------------------------------|
| `main`                                     | Mirror of `upstream/main`. Fast-forwarded only — never commit here.     |
| `fix/<short-slug>` *(one per upstream PR)* | A single, atomic, upstream-shaped fix branched off `upstream/main`.     |
| `fts/integration`                          | Rebase of `upstream/main` + every currently-needed `fix/*` branch on top. This is what `Cargo.toml` pins. |

The integration branch is the *only* place we combine fixes. Every
individual fix lives on its own branch so we can open a PR for it without
dragging unrelated changes along.

## Local checkout

```sh
# One-time
git clone git@github.com:FastTrackStudios/blitz ~/Development/FastTrackStudio/forks/blitz
cd ~/Development/FastTrackStudio/forks/blitz
git remote add upstream https://github.com/DioxusLabs/blitz.git
git fetch upstream
```

## Sync `main` to upstream

```sh
cd ~/Development/FastTrackStudio/forks/blitz
git fetch upstream
git checkout main
git merge --ff-only upstream/main
git push origin main
```

## Add a new fix

1. Branch off `upstream/main`, **not** `fts/integration`:
   ```sh
   git fetch upstream
   git checkout -b fix/refcell-borrow-on-set-focus upstream/main
   ```
2. Make the smallest possible change. One logical fix per branch.
   - Conventional-Commits style commit message (`fix(dioxus-native-dom): ...`).
   - Reference the upstream issue in the body if one exists; if not, file
     one first and link it.
   - Add a regression test under the relevant package's `tests/` (or an
     example under `examples/`) when feasible — reviewers will ask.
3. Push:
   ```sh
   git push -u origin fix/refcell-borrow-on-set-focus
   ```
4. Open a PR from `FastTrackStudios:fix/refcell-borrow-on-set-focus` →
   `DioxusLabs:main`.
5. **Restack `fts/integration`** so our app picks up the fix:
   ```sh
   git checkout fts/integration
   git fetch upstream
   git reset --hard upstream/main
   git merge --no-ff fix/refcell-borrow-on-set-focus
   # …repeat `git merge --no-ff` for every other still-open fix branch…
   git push --force-with-lease origin fts/integration
   ```
6. Bump the dep in `architect-ui` so cargo refetches:
   ```sh
   cargo update -p dioxus-native
   ```

## Drop a fix once it merges upstream

When a `fix/*` PR lands in upstream:

```sh
cd ~/Development/FastTrackStudio/forks/blitz
git fetch upstream

# Re-sync main to pick up the merged commit.
git checkout main
git merge --ff-only upstream/main
git push origin main

# Rebuild integration without that fix branch.
git checkout fts/integration
git reset --hard upstream/main
# …re-merge only the still-pending fix branches…
git push --force-with-lease origin fts/integration

# Optional: delete the merged fix branch.
git branch -D fix/refcell-borrow-on-set-focus
git push origin --delete fix/refcell-borrow-on-set-focus
```

Then in `architect-ui`:

```sh
cargo update -p dioxus-native
```

## Rules of thumb

- **Never** commit fts-ui-specific changes (renames, hard-coded paths,
  app-side workarounds) into `fix/*`. Those branches must be safe to PR
  upstream as-is.
- If an upstream-unfriendly hack is genuinely needed, put it on its own
  branch named `fts/<slug>` (note the `fts/` prefix instead of `fix/`)
  and merge it into `fts/integration` like any other branch — but skip
  the PR step.
- Keep `fts/integration` strictly == `upstream/main` + clean merges of
  branches. If conflicts arise during a restack, resolve them on the
  individual `fix/*` branch (rebase it onto the new upstream HEAD) rather
  than fixing conflicts in the merge commit on `fts/integration`.
- Run the workspace tests in the fork before pushing
  `fts/integration` so we don't ship a broken integration to architect-ui.

## Pinning strategy in `architect-ui`

`Cargo.toml` references `branch = "fts/integration"` so cargo refetches
on `cargo update -p dioxus-native`. If we ever need a hard pin (e.g. for a
release), swap `branch = "fts/integration"` for
`rev = "<commit-sha-from-fts/integration>"`.
