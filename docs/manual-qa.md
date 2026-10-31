# Manual QA

Run these in `npm run tauri dev` (with `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk`
exported) before each release. Use throwaway files, never real data.

## Safe Trash and Activity (M4)

Setup: `mkdir -p ~/MyMacStorage-QA && cd ~/MyMacStorage-QA && for i in 1 2 3; do head -c 5000000 /dev/urandom > file$i.bin; done`, then scan the home folder.

1. **Move from the Inspector.** In Space, open `MyMacStorage-QA`, select `file1.bin`, click Move to Trash.
   - The Review sheet lists one item with its path and size; focus starts on Cancel.
   - Confirm. The banner reads "Moved 1 item (5.0 MB) to the Trash".
   - Finder's Trash contains `file1.bin`. Space no longer lists it and the folder total dropped.
2. **Undo from the banner.** Click Undo. `file1.bin` is back in `~/MyMacStorage-QA` with the same contents
   (`shasum` before and after). Space shows it again.
3. **⌘⌫ and ⌘Z.** Select `file2.bin`, press ⌘⌫, confirm. Press ⌘Z: it comes back.
4. **Find, several at once.** In Find search `file`, ⌘-click two results, Move to Trash, confirm.
5. **Changed after the scan.** Select `file3.bin`, then in Terminal `echo x >> ~/MyMacStorage-QA/file3.bin`.
   Move to Trash: the Review sheet marks it "changed after the scan" and the move button is disabled.
6. **Protected paths.** Scan Entire disk, find `/System/Library` in Space: the Inspector shows a lock
   and Move to Trash is disabled.
7. **Name conflict.** Trash `file1.bin`, then `touch ~/MyMacStorage-QA/file1.bin`. In Activity, open the
   batch, Undo all: it reports "name taken". Click "Restore with a new name": `file1 (restored).bin` appears.
8. **Activity.** The weekly chart and totals include the batches above. Export CSV writes a file in
   Downloads and reveals it. Clear history asks first and leaves the Trash untouched.
9. **Empty Trash.** From the banner or the Overview Trash card: the dialog warns that the whole Trash is
   deleted. The first time, macOS asks to let MyMacStorage control Finder; allow it. Afterwards the Trash
   is empty and Activity shows those items as Emptied.
10. **Reconcile on launch.** Trash a file, quit MyMacStorage, empty the Trash in Finder, start MyMacStorage:
    Activity shows the item as Emptied.

Automated equivalent of steps 1-2 against the real Trash:
`cargo test -p mymacstorage-core real_trash -- --ignored` (moves three temp files to the Trash and back).
