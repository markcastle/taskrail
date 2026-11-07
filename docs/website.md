# The Task Rail website

What the site is for, what is on it, how it is built, and what it must never do. The same rule as the app: ask what Apple would do. One idea per page, nothing that needs explaining.

## What it is for

Three jobs, in this order.

1. **Make someone want it in ten seconds.** A visitor who has never heard of Task Rail sees what it is, sees it on a real desktop, and gets the one line that installs it.
2. **Be the front door for connecting things.** The blueprint registry is the reason most people never need an API key. The site shows what is already supported, and makes submitting a new one a thing people do.
3. **Hold the documentation.** The docs in the repository, rendered properly, so nobody reads raw Markdown on GitHub.

It is not a marketing site with a pricing page. The app is free and open source, and the site should feel like it: short, honest, no dark patterns, no newsletter popups, no cookie banner because there is nothing to consent to.

## Who it is for

- **Someone sent the link.** Wants to know what it is and whether it is safe to install. Reads the first screen, maybe the second, then installs or leaves.
- **Someone who already uses it.** Comes back to connect a new system, read the roadmap, or find a release note. Wants the Connect page and the changelog, nothing else in the way.
- **Someone who wants to contribute.** A blueprint, a bug, a pull request. Needs the docs and the submission path, and a sense that the project is alive.

## The pages

### Home

One screen tells the whole story. In order:

- The logo and the name.
- One sentence: *A task strip that lives on the edge of your screen, fed by the systems your work already lives in.*
- The rail, shown on a real desktop, light and dark, with the rest of the screen blurred so the eye lands on the strip. A short looping video of a flick-to-complete and a drag is better than a still, if it is under five seconds and silent.
- The install line for the visitor's own platform, detected, with the other one a click away. A copy button. Under it, in small type: *Free. Open source. Nothing phones home.*

Below the fold, three short sections, each one screenshot and two sentences:

- **Your day at a glance.** The date, the count, what is up next, your priorities in your order.
- **Gestures, not menus.** Drag, pull, flick, tap.
- **Any system with an MCP server.** Known ones connect from a blueprint. New ones, a model works out once, then it is out of the loop.

Then a strip of the systems with blueprints, as logos or names, linking to Connect. Then the footer.

### Download

For people who want the file rather than the line. Detects the platform, shows the installer for it, lists the others, and links the release notes. Says plainly that the builds are not yet code-signed and what that means on first run, until they are. Pulls the list from GitHub Releases at build time so it is never stale.

### Connect

The blueprint registry, rendered. One card per system: name, what the profile pulls, what it can do (read, change status, create), who contributed it. A search box. At the top, the two-sentence story: *Known systems connect without a model or a key. Not here yet? Connect it once and press Submit for review.*

At the bottom, how submission works, in four steps, linking to the issue form. This page is built from `blueprints/registry.json` at build time, so merging a blueprint updates the site on the next deploy.

### Docs

The repository's `docs/` folder rendered as a site section with a left navigation: Connecting your work, Developing, Architecture, Roadmap, Blueprints. Same source as the repo, so there is one copy of the truth. Headings become anchors, code blocks get copy buttons, and that is all the cleverness it needs.

### Changelog

Every release, newest first, with its notes and links to its installers. Built from GitHub Releases. This is also what the app's **Release notes** link should point at once the site exists, rather than the GitHub release page.

### Security and privacy

One page, short, for the person deciding whether to trust it:

- What leaves the machine, and when: nothing, except the systems you connect and, during discovery only, the model you chose.
- Where keys and sign-ins live: the operating system's credential store, never a file.
- How updates are verified: every build signed, the app refuses anything else.
- The code is open; here is the crate that talks to a model, and it is small.

The same content as the README's API key section, given room.

### About

Who makes it, why, and how to get in touch. A paragraph, not a page. Links to GitHub, issues, and the roadmap.

## Design

- **The app's look, scaled up.** The same blue, the same flat surfaces, the same type. A visitor should feel the site and the app were made by the same hands. Light and dark, following the system.
- **The rail is the hero.** Every page that shows the product shows the rail on a desktop, never a floating screenshot on white.
- **Short.** Every piece of copy gets cut by a third after it is written. If a sentence explains a feature, show the feature instead.
- **One accent, used sparingly.** Blue for the install button and links. Nothing else competes.
- **No motion for its own sake.** The one looping video on the home page, and that is all.

## How it is built

- **Static.** There is nothing to log into and nothing to serve dynamically, so the site is files. Astro is the right tool: it renders Markdown from the repo's `docs/` folder, builds pages from JSON at build time, and ships no JavaScript it does not need. Plain HTML would also do; Astro saves rewriting the docs renderer.
- **Lives in the repository** under `site/`, so a change to a doc, a blueprint or a release updates the site without anyone remembering to. One source of truth for everything.
- **Deployed on push** by a GitHub Actions workflow, to Cloudflare Pages or GitHub Pages. Cloudflare Pages is the better fit: a custom domain, a CDN, redirects in a text file, and analytics that need no cookie banner.
- **The install scripts get short addresses.** `taskrail.uk/install.ps1` and `taskrail.uk/install.sh` redirect to the raw files on GitHub (redirects, not copies, so a fix in the repository is live at once), so the one-liners in the README, the release notes and the site become shorter and survive a move of the repository.
- **Data at build time, not run time.** The blueprint registry, the docs, and the releases are read when the site is built. The site never calls GitHub from a visitor's browser.

## The domain

`taskrail.uk`, already registered and live. The name without a hyphen, matching the repository.

## What it must never do

- Track visitors beyond page counts. No third-party scripts, no fingerprinting, no cookies.
- Ask for an email address.
- Say anything the app does not do. The roadmap is the place for intentions, and it is labelled as such.
- Overstate the Mac story. Until the macOS build has had real time on real machines, the download page asks Mac users for feedback.

## Order of work

1. Home and Download, with the install lines and the first screenshots. This alone is worth shipping.
2. The deploy workflow and the domain, with the short install addresses.
3. Connect, from the registry.
4. Docs and Changelog, from the repo and the releases.
5. Security and About.

Each step is a day or less. The first two can be live within a week of starting.
