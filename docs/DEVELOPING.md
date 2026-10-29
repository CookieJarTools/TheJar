# Working on The Jar

## Adding an app

Add it to [`apps.json`](../apps.json) and push. The Jar reads that file from this repo on every check, so there's no need for a new release of The Jar. A copy is built into The Jar for when it's offline.

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
| `linux.desktop_name` | The menu entry's file name. It should match the app's Wayland app ID so its window gets the right icon. The Jar also looks for an existing menu entry with this name, or the app's `name`, to find copies it didn't install. Defaults to `id`. |
| `linux.categories` | Menu categories, such as `Development;`. |
| `linux.protocols` | URL schemes the app opens, without `://`. |

An app's releases need:

- a tag that's a version number, such as `v1.2.3`
- on Windows, an NSIS installer `.exe` (electron-builder's default works, and a file with `Setup` in its name is preferred)
- on Linux, an x86_64 `.AppImage`

The Jar picks the newest release that has a file for the system it's on, so an app can ship its Linux files first and its Windows installer later. Windows users stay on the previous version until the installer is attached.

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
- **current** releases the version already in `Cargo.toml`, after you've set it by hand

The [release workflow](../.github/workflows/release.yml) tests on Windows and Linux and builds `TheJar-Setup-x.y.z.exe` and `TheJar-x.y.z.AppImage` with the new version. Once everything has built, it commits the version to `main`, tags it, and publishes the release with notes generated from the commits since the last one. If any step fails, nothing is pushed. Leave **Mark it as a pre-release** ticked to match the other CookieJarTools apps. Every copy of The Jar offers the update on its next check.

From a terminal, `gh workflow run release.yml -f bump=patch` does the same. Run `git pull` afterwards, since the workflow commits the version change to `main`.

Pushing a tag such as `v0.2.0` by hand still works too: the workflow builds that tag and attaches the files to its release. `scripts/bump-version.py` is what sets the version, if you want to bump it locally without releasing.
