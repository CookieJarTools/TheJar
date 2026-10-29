# The Jar

The Jar installs and updates the CookieJarTools desktop apps on Windows and Linux. Right now that's [CrumbTree](https://github.com/CookieJarTools/CrumbTree) and [GameBridge](https://github.com/CookieJarTools/GameBridge), and new apps appear in it as soon as they're added to [`apps.json`](apps.json).

It's a single small Rust program with an [egui](https://github.com/emilk/egui) window, so it opens straight away and doesn't need a browser engine.

## Install

**Windows:** download `TheJar-Setup-x.y.z.exe` from [Releases](https://github.com/CookieJarTools/TheJar/releases) and run it. It installs for your user account only, so it doesn't ask for admin rights. The installer isn't signed yet, so SmartScreen may stop it: choose **More info**, then **Run anyway**.

**Linux:** download `TheJar-x.y.z.AppImage`, make it executable and run it:

```sh
chmod +x TheJar-*.AppImage
./TheJar-*.AppImage
```

The first time it runs, it adds itself to your app menu.

## What it does

- Lists each app with the version you have and the newest one available.
- Installs, updates, opens and uninstalls them. Uninstalling keeps the app's own settings and data.
- Checks for updates when it opens and every six hours while it's open. Press F5 to check now.
- Shows each release's notes before you update.
- Updates itself from this repo's releases.

### Which release it picks

The newest release that has a file for your system. Pre-releases count, since most CookieJarTools releases are marked as pre-releases, but you can turn that off in Settings. A release with no file for your system is skipped. GameBridge 0.4.0, for example, only has Linux files so far, so Linux gets 0.4.0 while Windows stays on 0.3.1 until its installer is attached.

Every download is checked against the SHA-256 checksum GitHub publishes for it, and thrown away if it doesn't match.

### How apps are installed

**On Windows** The Jar runs the app's own installer silently for your user, which is the same as running it yourself. Apps show up in the Start menu and in Settings › Apps, and apps you installed by hand before are picked up too. If an app is open when you update it, The Jar asks first, because the installer closes it.

**On Linux** apps are AppImages kept in `~/.local/share/the-jar/apps`, with menu entries and icons written to `~/.local/share/applications`. Link handlers such as `crumbtree://` are registered too. An update swaps the file, so a copy that's already running carries on until you reopen it. The Jar never overwrites or deletes a menu entry it didn't write.

## Adding an app

Add it to [`apps.json`](apps.json) and push. The Jar reads that file from this repo on every check, so there's no need for a new release of The Jar. A copy is built into The Jar for when it's offline.

```json
{
  "id": "crumbtree",
  "name": "CrumbTree",
  "tagline": "A fast, friendly git client with a vertical commit graph.",
  "repo": "CookieJarTools/CrumbTree",
  "icon": "https://raw.githubusercontent.com/CookieJarTools/CrumbTree/HEAD/resources/icon.png",
  "windows": { "product_name": "CrumbTree" },
  "linux": {
    "desktop_name": "crumbtree",
    "categories": "Development;RevisionControl;",
    "protocols": ["crumbtree"]
  }
}
```

| Field | What it's for |
| --- | --- |
| `id` | Short and lowercase. Used for folder names and on the command line. |
| `name`, `tagline` | Shown in the list. |
| `repo` | The public GitHub repo whose releases hold the installers. |
| `icon` | A PNG. 256×256 or larger looks best. |
| `windows.product_name` | The name the installer registers in Settings › Apps. Defaults to `name`. |
| `windows.exe` | The program inside the install folder. Defaults to `<product_name>.exe`. |
| `windows.asset`, `linux.asset` | Which release file to use, with `*` as a wildcard. Only needed if the defaults pick the wrong one. |
| `linux.desktop_name` | The menu entry's file name. It should match the app's Wayland app ID so its window gets the right icon. Defaults to `id`. |
| `linux.categories` | Menu categories, such as `Development;`. |
| `linux.protocols` | URL schemes the app opens, without `://`. |

An app's releases need:

- a tag that's a version number, such as `v1.2.3`
- on Windows, an NSIS installer `.exe` (electron-builder's default works, and a file with `Setup` in its name is preferred)
- on Linux, an x86_64 `.AppImage`

## Command line

Everything in the window can also be done from a terminal:

```sh
thejar list                 # what's installed and what's available
thejar install crumbtree    # install, or update if it's already there
thejar update               # update every installed app
thejar uninstall gamebridge
thejar open crumbtree
thejar self-update
```

## Building

You need [Rust](https://rustup.rs) 1.88 or newer.

```sh
cargo build --release
cargo test
```

`THEJAR_API_BASE` and `THEJAR_CATALOG_URL` point The Jar at another GitHub API and app list, which is handy for testing against a local server. Plain-HTTP downloads are only allowed while `THEJAR_API_BASE` is set.

## Releasing The Jar

On GitHub, open **Actions › Release › Run workflow**, pick which part of the version goes up, and press **Run workflow**:

- **patch** for fixes (0.1.0 → 0.1.1), **minor** for new features (0.1.1 → 0.2.0), **major** for big changes (0.2.0 → 1.0.0)
- **current** releases the version already in `Cargo.toml`, for the first release or after you've set it by hand

The [release workflow](.github/workflows/release.yml) tests on Windows and Linux and builds `TheJar-Setup-x.y.z.exe` and `TheJar-x.y.z.AppImage` with the new version. Once everything has built, it commits the version to `main`, tags it, and publishes the release with notes generated from the commits since the last one. If any step fails, nothing is pushed. Leave **Mark it as a pre-release** ticked to match the other CookieJarTools apps. Every copy of The Jar offers the update on its next check.

From a terminal, `gh workflow run release.yml -f bump=patch` does the same. Run `git pull` afterwards, since the workflow commits the version change to `main`.

Pushing a tag such as `v0.2.0` by hand still works too: the workflow builds that tag and attaches the files to its release. `scripts/bump-version.py` is what sets the version, if you want to bump it locally without releasing.

## Licence

The Jar is under the [PolyForm Shield License 1.0.0](LICENSE.md). It bundles the [Inter](https://rsms.me/inter/) typeface, which is under the [SIL Open Font License](assets/fonts/OFL.txt).
