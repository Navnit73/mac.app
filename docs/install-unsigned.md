# Installing MyMacStorage (text for the website's download page)

Copy this to the download / thank-you page. It applies while the app is not signed with an Apple
Developer ID (see docs/release.md for the signed setup).

---

## Install MyMacStorage

1. Open **MyMacStorage.dmg** and drag **MyMacStorage** into **Applications**.
2. Open **MyMacStorage** from Applications. macOS shows *"Apple could not verify 'MyMacStorage' is free
   of malware"*. Click **Done** (not Move to Trash).
3. Open **System Settings → Privacy & Security**, scroll down to *"MyMacStorage was blocked to protect
   your Mac"*, and click **Open Anyway**. Enter your Mac password if asked.
4. Click **Open Anyway** once more. From now on MyMacStorage opens normally.

You only do this once, and again after installing an update.

**Why?** MyMacStorage is sold directly by us, not through the App Store, and is not registered with
Apple's paid notarization service. The warning means Apple has not scanned it, not that something is wrong.
MyMacStorage runs entirely on your Mac, never uploads your files, and only ever moves files to the
Trash.

**"Open Anyway" isn't there?** It appears only for about an hour after step 2. Open the app again,
then go back to Privacy & Security.

**Prefer Terminal?** This removes the download flag so the warning is not shown:

```sh
xattr -dr com.apple.quarantine /Applications/MyMacStorage.app
```

**Requirements:** macOS 14 Sonoma or later, Apple Silicon Mac (M1 or later).
