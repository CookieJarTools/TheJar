# The Jar

The Jar installs the CookieJarTools desktop apps on Windows and Linux and keeps them up to date. Right now that's [CrumbTree](https://github.com/CookieJarTools/CrumbTree) and [GameBridge](https://github.com/CookieJarTools/GameBridge), and new apps appear in it on their own as they're released.

It's small and opens straight away.

## Install

**Windows:** download `TheJar-Setup-x.y.z.exe` from [Releases](https://github.com/CookieJarTools/TheJar/releases) and run it. It installs for your user account only, so it doesn't ask for admin rights. The installer isn't signed yet, so SmartScreen may stop it: choose **More info**, then **Run anyway**.

**Linux:** download `TheJar-x.y.z.AppImage`, make it executable and run it:

```sh
chmod +x TheJar-*.AppImage
./TheJar-*.AppImage
```

The first time it runs, it adds itself to your app menu.

## Using it

The **Library** lists every app with the version you have and the newest one available. Each app has **Install**, **Update** or **Open** next to it. Click an app to see what's new in its latest release and where it's installed, or to uninstall it.

The Jar checks for updates when it opens and every six hours while it's open. Press **F5** to check now.

When there's a new version of The Jar itself, a card at the bottom of the sidebar says so. Click **Update**, then **Restart now**.

In **Settings** you can:

- turn pre-releases off, if you only want stable versions (most CookieJarTools releases are pre-releases, so they're on by default)
- stop The Jar checking for updates every time it opens

## Good to know

- **Which version you get:** the newest release that has a file for your system. A release with nothing for your system yet is skipped until its file is added.
- **Downloads are checked** against the checksum GitHub publishes for each file, and thrown away if they don't match.
- **Uninstalling keeps your settings and data.** Each app keeps those in its own folder, and The Jar leaves them alone.
- **Offline,** The Jar shows what it found last time.

**On Windows** The Jar runs each app's own installer for you, the same as if you'd run it yourself. Apps appear in the Start menu and in Settings › Apps, and apps you installed before you had The Jar are picked up too. If an app is open when you update it, The Jar asks first, because updating closes it.

**On Linux** apps are AppImages kept in `~/.local/share/the-jar/apps`, with menu entries and icons in `~/.local/share/applications`. Links such as `crumbtree://` open the right app. If an app is running when it updates, it carries on as the old version until you reopen it. The Jar never changes or removes a menu entry it didn't make.

## Command line

Everything in the window can also be done from a terminal:

```sh
thejar list                 # what's installed and what's available
thejar install crumbtree    # install, or update if it's already there
thejar update               # update every installed app
thejar uninstall gamebridge
thejar open crumbtree
thejar self-update          # update The Jar itself
```

## Licence

The Jar is under the [PolyForm Shield License 1.0.0](LICENSE.md). It bundles the [Inter](https://rsms.me/inter/) and [Fredoka](https://github.com/hafontia/Fredoka-One) typefaces, both under the SIL Open Font License ([Inter](assets/fonts/Inter-OFL.txt), [Fredoka](assets/fonts/Fredoka-OFL.txt)).
