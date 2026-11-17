# Release and auto-update playbook

How Task Rail ships: one button cuts a release, the app updates itself, and a second button undoes a bad release. This page explains how it is put together and how to set the same thing up in another project.

It is written to be followed by someone who has never seen this repository. Where a step is specific to Tauri it says so, and what to do in its place.

## What you end up with

- **Releasing is one click.** Open Actions, pick Release, press Run. No terminal, no version number to choose.
- **The changelog decides the version.** Something new makes a minor release; fixes make a patch.
- **Release notes write themselves.** They are the changelog section for that version, written for the people using the app.
- **A release is never half there.** It stays a draft until every installer is attached, then publishes.
- **The app keeps itself current.** It checks every hour, verifies a signature, and either offers the update or installs it.
- **A bad release can be undone from a button,** including for people who already installed it.
- **A broken main cannot be released.** The button refuses unless CI passed on that exact commit.

## The idea in one paragraph

Write what changed into `CHANGELOG.md` as the work lands. That one habit drives everything else: the script reads it to choose the version number, stamps it with the version and date, and the workflow copies that section into the GitHub Release. The installed app reads a small signed file attached to the latest release to learn there is something newer. Nobody types a version, writes release notes at release time, or uploads a file by hand.

## How a release flows

1. A change lands on `main` with a line under **Unreleased** in the changelog.
2. CI runs on that commit and passes.
3. Someone presses **Release**.
4. The workflow works out the version, stamps the changelog, sets the version in the project files, commits, tags and pushes.
5. It creates the GitHub Release as a **draft** with the changelog section as its notes.
6. It builds the installers on Windows and macOS, signs the update files, and uploads them to the draft.
7. It checks everything is attached and **publishes** the draft. GitHub now calls it "latest".
8. Within the hour each running app fetches `latest.json`, sees a higher version, and offers or installs it.

## The pieces

| File | What it does | Portable as is? |
| --- | --- | --- |
| `CHANGELOG.md` | Source of truth for notes and for the version number | Yes |
| `scripts/changelog.mjs` | Reads a version's section, stamps a release, works out the next version | Yes, change the repository URL |
| `scripts/changelog.test.mjs` | Tests the numbering rule | Yes |
| `scripts/release.mjs` | Stamps the changelog, sets the version in the project files, commits, tags | Change the list of files it rewrites |
| `.github/workflows/release.yml` | The Release button: prepare, build, publish | Change the build job for your stack |
| `.github/workflows/rollback.yml` | The Roll back button | Yes |
| `.github/workflows/ci.yml` | The gate the Release button checks | Yours will differ |
| `src-tauri/tauri.conf.json` | Updater public key and where to look for updates | Tauri only |
| `src-tauri/src/update.rs` | When the app checks, and what it does about an update | Tauri only, the policy is portable |
| `install.ps1`, `install.sh` | One-line install and update from the latest release | Change the repository and app name |

Only Node is needed to run the scripts. They have no dependencies.

## Part 1: The changelog

### The format

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), with an `Unreleased` section always at the top:

```markdown
## [Unreleased]

### Added

- **Task Rail starts when you sign in.** A rail you have to remember to open is a to-do list with extra steps...

### Fixed

- The date at the top keeps up.

## [0.11.1] - 2026-11-12
```

Three rules make it work:

- **Every change a person would notice gets a line as it lands,** in the same commit as the change. Not at release time, when nobody remembers.
- **Write for the person using the app.** Say what they will see, not which function moved. These lines become the release notes word for word.
- **File it under the right heading.** The heading sets the version number, as the next section explains.

### The heading decides the version

| Unreleased lists | The release is | From 0.11.1 |
| --- | --- | --- |
| Something **Added** or **Removed** | A minor release | 0.12.0 |
| Only **Changed** and **Fixed** | A patch | 0.11.2 |

A new ability goes under Added even when it is small. A redesign of something that already exists goes under Changed. The rule is one function, `nextVersion` in `scripts/changelog.mjs`, with tests.

You can always overrule it by typing a version. That is how 1.0.0 happens, because leaving 0.x is a judgement no heading can make.

### What the script does

```bash
node scripts/changelog.mjs next 0.11.1
```

Prints the next version on standard output and the reason on standard error, so a script can capture the number and a person can read why.

```bash
node scripts/changelog.mjs release 0.12.0
```

Renames `Unreleased` to `[0.12.0] - <today>`, opens a fresh `Unreleased` holding "Nothing yet.", and adds the compare link. It refuses if `Unreleased` is empty, because a release with nothing to read about should not go out.

```bash
node scripts/changelog.mjs section 0.12.0
```

Prints that version's notes. The workflow uses this for the release body.

```bash
node scripts/changelog.mjs note "Rolled back to the code of 0.11.1. It crashed on empty feeds."
```

Adds a line under Changed in `Unreleased`. The rollback workflow uses it.

## Part 2: The release command

`scripts/release.mjs` is the single place a release is cut. The Release button runs it, and so can a person, so the two routes cannot drift apart.

```bash
npm run release
```

In order, it:

1. Refuses if the working tree has uncommitted changes.
2. Fetches tags, so it counts from releases cut on GitHub and not from the last one this checkout knows about.
3. Works out the version from the changelog, unless one was given: `npm run release -- 1.0.0`.
4. Stamps the changelog.
5. Sets the version in every file that carries it.
6. Commits as `Release vX.Y.Z` and creates the tag `vX.Y.Z`.

**To port it:** change the `rewrite(...)` lines to the files your project keeps its version in. Task Rail has three, plus a lock file:

| File | Why |
| --- | --- |
| `package.json` | The JavaScript side |
| `src-tauri/tauri.conf.json` | What the installer and the updater read |
| `Cargo.toml` | The Rust workspace |
| `Cargo.lock` | Updated with `cargo update --workspace` so it agrees |

If two of these disagree, the updater will compare the wrong numbers. Setting them in one place is the point.

## Part 3: The Release workflow

`.github/workflows/release.yml` has three jobs.

### How it can start

```yaml
on:
  workflow_dispatch:          # the button
    inputs:
      version:
        description: 'Leave blank and the changelog decides. Or type a version.'
        required: false
        type: string
      dry_run:
        description: 'Rehearse: do everything except push, build and publish'
        type: boolean
        default: false
  workflow_call:              # the rollback workflow calls it
    inputs:
      version: { required: true, type: string }
      skip_ci_check: { type: boolean, default: false }
  push:
    tags: ['v*']              # a tag pushed by hand still works

permissions:
  contents: write

concurrency:
  group: release
  cancel-in-progress: false   # one release at a time, and never cancel one
```

### Job 1: prepare

Runs on Linux and takes seconds. It decides, checks, cuts and creates the draft.

1. **Decide the version.** Blank input means ask the changelog. Then refuse anything that is not `x.y.z`, is not higher than the highest existing tag, already exists, or is not being cut from `main`.
2. **Check CI.** Look up the CI run for this exact commit and refuse unless it completed successfully:

    ```bash
    gh run list --workflow ci.yml --commit "$GITHUB_SHA" --limit 1 --json status,conclusion
    ```

3. **Cut the release** by running the release command with a bot identity, then push the commit and the tag.
4. **Write the notes** from the changelog section.
5. **Create the release as a draft:**

    ```bash
    gh release create "v$version" --draft --title "Task Rail v$version" --notes-file notes.md
    ```

It also writes the version and the reason to the run's summary page, so the number is visible without opening a log.

### Job 2: build

One matrix entry per platform. Each checks out the **tag**, not the branch, builds, and uploads to the draft.

```yaml
- uses: actions/checkout@v4
  with:
    ref: refs/tags/v${{ needs.prepare.outputs.version }}

- uses: tauri-apps/tauri-action@v0
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
    TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
  with:
    includeUpdaterJson: true
    tagName: v${{ needs.prepare.outputs.version }}
    releaseDraft: true      # this is how it finds the draft that prepare made
    prerelease: false
```

`releaseDraft: true` matters. With it, the action searches the release list and finds the existing draft. Without it, it looks the release up by tag, which cannot see drafts, and tries to create a second one.

**If you are not on Tauri:** replace this step with your own build, then `gh release upload "v$version" <files>`. Everything else stays the same.

### Job 3: publish

Runs only after every build succeeded. It lists the assets on the draft, refuses if an installer or `latest.json` is missing, and then opens it:

```bash
gh release edit "v$version" --draft=false --latest
```

If a build failed, this job never runs and the release stays a draft. Nobody is ever offered a release with nothing to download.

### Rehearse

Ticking **Rehearse** runs the whole of prepare except the push and the draft, then stops. It is the safe way to ask "what number would the next release be?" and to test a change to the workflow itself. It can run from any branch.

## Part 4: Rolling back

`.github/workflows/rollback.yml` takes two inputs: the last good version, and one sentence on why.

The important thing to understand first: **an updater only ever installs a higher version.** You cannot move an app from 0.9.0 back to 0.8.1 by pointing it at 0.8.1. A rollback is therefore a new, higher release that holds the old code.

The workflow does three things:

1. **Withdraws every release newer than the good one** by marking each a pre-release. GitHub's "latest" skips pre-releases, so it points at the good version again immediately. New installs get the good build, and apps that had not updated yet stop being offered the bad one.

    ```bash
    gh release edit "v$bad" --prerelease
    ```

2. **Puts the good version's code back on `main`** as one commit, keeping the release tooling at its current state, and adds a line to the changelog saying so and why.

    ```bash
    git read-tree --reset -u "v$good"
    git checkout HEAD -- .github scripts CHANGELOG.md
    ```

3. **Calls the Release workflow** for the next patch number, with the CI check skipped because that exact code was released before.

Work that landed after the good version is gone from `main` after this. It is not lost: revert the "Roll back to" commit to bring it back, fix it, and release again.

## Part 5: The auto-updater

This part is Tauri 2 with `tauri-plugin-updater`. The policy at the end applies to any updater.

### One-time setup

**1. Make a signing key.** Keep the private half out of the repository.

```bash
npx tauri signer generate -w ~/.tauri/myapp.key
```

It asks for a password and writes two files: the private key, and `myapp.key.pub`.

**2. Put the public key and the feed address in `tauri.conf.json`:**

```json
{
  "plugins": {
    "updater": {
      "pubkey": "<contents of myapp.key.pub>",
      "endpoints": ["https://github.com/OWNER/REPO/releases/latest/download/latest.json"]
    }
  },
  "bundle": {
    "createUpdaterArtifacts": true
  }
}
```

That address is the whole distribution system. GitHub redirects `releases/latest/download/<file>` to that file on whichever release is currently "latest". There is no server to run.

**3. Give the private key to the workflow as two secrets.** Set them from the file, not by pasting:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo OWNER/REPO < ~/.tauri/myapp.key
```

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo OWNER/REPO
```

Pasting the key into the web form tends to drop its line break, and the build then fails with "Missing comment in secret key".

**4. Add the plugin to the app:**

```toml
tauri-plugin-updater = "2"
```

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_updater::Builder::new().build())
```

Calling it from Rust needs no extra permission entries.

### What the app does

The whole of it is `src-tauri/src/update.rs`, about ninety lines.

- **Check twenty seconds after start, then every hour,** for as long as the app runs. Twenty seconds keeps it out of the way of startup. Hourly means a release reaches people the same working day. Four hours, the first value tried, was too slow.
- **If something newer exists,** either install it at once or tell the page, depending on one setting.
- **Install** means download, verify the signature against the public key, run the installer, and restart into the new version.

```rust
let updater = app.updater()?;
if let Some(update) = updater.check().await? {
    release_anything_the_os_holds(app);   // see the traps below
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart();
}
```

### The policy worth copying

- **Updating automatically is off by default.** An update that changes things under someone is a surprise. Offer it, with a switch for people who want it hands-off.
- **The offer is one line and one button:** "Task Rail 0.12.0 is available." and **Install now**, with a way to put it away. It waits; it never interrupts.
- **Do not explain the update in the app.** Link to the release page, which already holds the changelog section.
- **Settings has "Check for updates"** that answers in one line: up to date, or the version available with a link and an install button.
- **Log every check.** "up to date", "update 0.12.0 available", "update check failed: ...". When someone says the app is not updating, that line answers it.

### What signing does and does not do

The updater key proves an update came from you. The app refuses anything that does not verify, so a tampered download cannot install.

It is not operating-system code signing. Without a Windows certificate and an Apple Developer ID, a person who downloads an installer by hand still meets SmartScreen or Gatekeeper on first run. Updates through the app are unaffected. Task Rail does not have those certificates yet; when it does they go in as repository secrets and `tauri-action` picks them up.

### If the key is lost

Generate a new pair, put the new public key in `tauri.conf.json`, update the two secrets, and ship a release. Apps already installed will not accept updates signed with the new key, so that one release has to be installed by hand. Back the private key up somewhere that is not the repository.

## Part 6: One-line install

Two short scripts read the latest release and install it. The same line updates.

```powershell
irm https://example.com/install.ps1 | iex
```

```bash
curl -fsSL https://example.com/install.sh | sh
```

What they do, and why each step is there:

| Step | Windows | macOS |
| --- | --- | --- |
| Find the installer | Ask the GitHub API for the latest release, pick the asset ending `-setup.exe` | Same, pick the `.dmg` |
| Clear the download warning | `Unblock-File` | Remove the quarantine attribute |
| Stop a running copy | `Stop-Process` | `osascript -e 'quit app ...'` |
| Install without prompts | Run the installer with `/S`, per user, no administrator | Copy the app into `/Applications` |
| Start it | Launch the installed executable | `open` the app **by path** |

Match assets by their ending, never by full name. The build tools change spaces to dots in file names.

Serve the scripts from your own short address if you have one, and make it a redirect to the files in the repository so there is one copy to maintain.

## Part 7: CI is the gate

The Release button trusts CI completely, so CI has to mean something. Task Rail's has three jobs:

- **Core:** formatting, lints and unit tests on Linux. Fast, fails first.
- **UI:** type check, lint, the release script's own tests, and a browser test.
- **Desktop:** a full build and test on Windows and on macOS, after the first two pass.

Two details that save trouble:

- **Test the release tooling in CI.** The numbering rule is code, and a wrong number is a bad release. `npm run scripts:test` runs it.
- **Do not let a flaky download fail the gate.** Installing a test browser's optional system packages can fail when a package mirror is down. Fall back to the browser alone:

    ```bash
    npx playwright install --with-deps chromium || npx playwright install chromium
    ```

## The traps

Most of these cost a failed or confusing release before they were understood; the rest are guarded against in the scripts for a reason. They are the most valuable part of this page.

| What you see | Why | What to do |
| --- | --- | --- |
| A tag pushed by the workflow starts no build | GitHub does not start workflows for events made with the workflow's own token | Build in the same workflow run, after the push |
| "Resource not accessible by integration" when creating the release, though permissions are right | The workflow token may only create a release for a commit that matches the head of `main` in its workflow files. The release was being created fifteen minutes after the tag, and `main` had moved | Create the release as a draft the moment the tag exists |
| A published release with no installers on it | The release was created before the builds finished, and one failed | Draft first, publish last, and check the assets before publishing |
| "Missing comment in secret key" during the build | The signing key was pasted into the secrets form and lost its line break | `gh secret set NAME < file` |
| The build creates a second release, or cannot find the draft | Looking a release up by tag does not see drafts | `releaseDraft: true`, so the action searches the list |
| The app ignores an older version you pointed it at | Updaters only install a higher version | A rollback is a new, higher release of the old code |
| The next release number is wrong when cut from a laptop | The checkout has not fetched tags made on GitHub | Fetch tags before computing the version |
| CI never ran on the commit being released | Commits pushed by the workflow do not trigger CI | Check CI on the commit a person pushed; skip the check for a rollback |
| The Mac installer fails on its last line | `open -a "App Name"` looks the app up by name, and a just-copied app is not always registered yet | Open it by path |
| The install script cannot find the installer | The build changed spaces in the file name to dots | Match by ending: `*-setup.exe`, `*.dmg` |
| After an update on Windows, something the app held is still held | The installer ends the process without the normal exit path | Release it yourself before calling install |
| Release notes are a wall of commit messages | They were generated from commit headlines | Use the changelog section |
| Nobody is sure what number to type | A rule that lives in someone's head | Let the changelog decide |

## Porting checklist

Do these in order. Each one works before the next begins.

1. **Start the changelog.** Create `CHANGELOG.md` with an `Unreleased` section and one section for where the project is now. Agree the rule: every noticeable change gets a line in the same commit.
2. **Copy `scripts/changelog.mjs` and its test.** Change the repository URL at the top. Run the test.
3. **Copy `scripts/release.mjs`.** Change the files it rewrites. Add `"release": "node scripts/release.mjs"` to `package.json`. Try it in a scratch clone and look at the commit it makes.
4. **Make sure CI is real** and add the script's test to it.
5. **Copy `release.yml`.** Change the app name, the CI workflow file name, the build job, and the list of assets the publish job requires.
6. **Rehearse.** Run the workflow with Rehearse ticked and read its log.
7. **Set up the updater:** generate the key, add the public key and endpoint, set the two secrets from files, add the plugin, and add the check loop.
8. **Cut a first real release** and install it by hand.
9. **Cut a second** and watch the first one find it. Until this works, the updater is not proven.
10. **Copy `rollback.yml`.** You will not be able to test the withdrawal step without a real release to withdraw, so read it carefully.
11. **Add the install scripts** if you want a one-line install.

## Day to day

**While working:** add a line under `Unreleased` in the same commit as the change.

**To release:** open Actions, pick Release, press Run, leave the version blank. The run's page says which number it chose and why. About twenty minutes later it is published.

**To check it worked:** the release page shows the installers and `latest.json`, and "latest" points at the new version. In the app, Settings has Check for updates.

**If a release is bad:** open Actions, pick Roll back, give the last good version and a sentence on why.

**If the button refuses,** it says why in one line: CI still running, CI failed, nothing under Unreleased, or the version is not higher than the last.

## Not done yet

- **Operating-system code signing.** A Windows certificate and an Apple Developer ID, so a hand-downloaded installer does not meet a warning.
- **Linux builds.** The matrix has Windows and macOS only.
- **A staged rollout.** Every running app is offered a release within the hour. There is no way to offer it to a few first.
