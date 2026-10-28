# Backups and transfer

[Back to README](../README.md)

## Cloud Backup

The **Cloud Backup** tab saves existing user configuration files through rclone, with Google Drive and Dropbox connection buttons and support for existing rclone accounts. Requires `rclone` on PATH. Sign-in opens in a terminal/browser; return and click **Refresh accounts** when finished. ArchEasy reads account names/types, not OAuth tokens, and does not copy the rclone credentials into its settings. The suggested list excludes the rclone config folder.

Choose an account, set a relative cloud folder (default `ArchEasy/ConfigBackups`), tick config paths or use **Add files… / Add folder…**, then choose **Save selected configs to cloud**. Nothing is selected by default. Review the exact paths and destination in the confirmation. Config files can contain passwords/tokens. Backups are uploaded as normal cloud files; encryption depends on the configured remote, such as an rclone crypt remote.

Each backup gets a unique dated folder containing `manifest.json` and `files/`, with source paths relative to your home directory. Files are staged privately, with a limit of 512 MiB and 100,000 entries. Linked paths and special files are excluded; nested links are counted as skipped. A file changing while it is copied aborts the snapshot. Close applications whose config you are backing up for a consistent result. Temporary snapshots are removed when the task finishes; failed uploads may leave a partial cloud folder, which is reported. Transfers run in the background; keep ArchEasy open until the status reports completion.

**List backups** reads the selected account/folder. **Download backup…** copies the selected snapshot into a new subfolder of a location you choose, refusing an existing destination. Downloads never apply configs automatically. Review `manifest.json` and copy the desired contents of `files/` into your home folder to restore. Existing cloud backups and local config files are not removed by these operations.

Validation uses a temporary local rclone alias remote for upload/list/download and a graphical test for selection, cancellation, upload confirmation, and backup listing. Google Drive/Dropbox browser authentication and live account transfers require your account sign-in; they are not exercised by automated tests.

References: [rclone account setup](https://rclone.org/commands/rclone_config_create/), [copy](https://rclone.org/commands/rclone_copy/), [listing](https://rclone.org/commands/rclone_lsjson/).

## Portable snapshots and file backups

Click **Snapshots & backups…** at the top of the main window, beside **System update** (also available in **System**). Click **How to create, transfer & restore a backup…** inside that dialog for the full step-by-step guide. Leave **Include installed app lists…** enabled to create a migration ZIP with the existing `my-system.sh` installer and managed configs. Tick the files/folders to include, or use **Add files… / Add folder…**. For a files-only backup, untick the app option. Save the ZIP outside all selected folders. Nothing is selected or backed up automatically.

For local/custom apps, include their project folders, `.local/bin`, desktop launchers and app data. The suggested paths include Projects, Documents, Pictures, Desktop, `.config`, `.local/share` and Flatpak user data (`.var/app`). Package-managed apps reinstall from current repositories; unavailable or vendor packages need their original sources. This is a portable app/file selection, not a disk image or an exact-version system clone.

Backups are private ZIP files (mode 0600) with a dated manifest, SHA-256 checksums, `home/` file contents, restore instructions and a standalone Python helper. Empty folders and ordinary file permissions are retained. Overlapping selections are deduplicated. Linked selections are rejected; nested links and special files are skipped and counted. Limits: 20 GiB and 200,000 entries. Files changing during copy abort the backup. Close apps before capture. The ZIP can contain credentials and is not encrypted; store it privately.

**Verify & unpack backup…** checks a snapshot and extracts into a new, uniquely named folder inside your chosen destination. Existing live files stay untouched. Review `home/` before copying desired files into your home. On another Arch computer, review and run `bash /path/to/unpacked/my-system.sh` to reinstall packaged apps and restore managed configs using the installer's rollback backups. Custom programs may require rebuilding or their original dependencies.

Without ArchEasy, extract only `restore-snapshot.py` from the ZIP, review it, then run:

```sh
python3 restore-snapshot.py restore /path/to/snapshot.zip /path/to/NEW_FOLDER
```

The destination must not exist. Unpacking validates paths, checksums and limits and never runs the included app installer. A failed unpack may leave an incomplete output folder for inspection. Keep ArchEasy open until a running backup or unpack finishes.


## Backup privacy

Snapshots and cloud capture exclude SSH/GPG directories, common browser profiles, rclone credentials, AWS/Kubernetes credentials and desktop key stores, including when selected through a parent folder. The complete policy is in [data/backup-exclusions.json](../data/backup-exclusions.json). These exclusions are enabled for every capture; there is currently no override in the UI. Other files may still contain secrets.

ZIP files, cloud uploads and local config rollback backups are **not encrypted**. Config rollback backups preserve the full before-state so recovery can restore the original files. Store them privately; use your own encryption before sharing a ZIP. Exclusion is not a substitute for encryption.

Snapshot saving falls back to exclusive file creation on filesystems without hard links, without replacing an existing file or symlink. A failed copy removes the partial output; interruption or power loss can leave an incomplete ZIP. Verify backups before relying on them. Cloud capture rejects source/staging overlap, including through symlinked staging ancestors.
