# DB Refresh — User Guide

DB Refresh keeps a local PostgreSQL database up to date with your backups. It finds database dumps in an S3 bucket, downloads them to your Mac, and restores them into a local database — for any project, as often as you like.

**Typical day:** open the app → **⇄ Sync database** → click **Refresh latest** on your database → confirm. That's it.

---

## 1. Before you start

You need:

- **A Mac with Apple Silicon** (M1 or newer) running **macOS 13** or later.
- **AWS access to the bucket** with your dumps, set up on your Mac in `~/.aws`:
  - with access keys: run `aws configure` in Terminal, or
  - with SSO: run `aws configure sso`, then `aws sso login --profile <name>` when your login expires.
- **PostgreSQL client tools** (`pg_restore`, `psql`). Install them with Homebrew: `brew install libpq`.
- **A local PostgreSQL server** with the database you want to refresh, and a user that can create and drop tables in it.

## 2. Install

1. Open the `.dmg` file and drag **DB Refresh** into **Applications**.
2. Open DB Refresh from Applications. The first time, macOS blocks it because it isn't from the App Store.
3. Open **System Settings → Privacy & Security**, scroll down, and click **Open Anyway**. You only do this once.

Prefer Terminal? Instead of steps 2–3, run once:
`xattr -dr com.apple.quarantine "/Applications/DB Refresh.app"`

The app has three tabs at the top: **↓ Download**, **⇄ Sync database** and **? Help** (this guide).

---

## 3. First-time setup (5 minutes)

### Add your database

1. Go to **⇄ Sync database** and click **Add database**.
2. Fill in the dialog:
   - **Quick fill** — paste a connection URL such as `postgres://app_user:secret@localhost:5432/app_db` and the fields below fill in for you. (The URL box clears itself because it contains your password.)
   - **Connection** — Host, Port, Database and User. The line under the fields shows where data will go, e.g. `app_user@localhost:5432/app_db`.
   - **Authentication** — your database password. Turn on **Remember password in macOS Keychain** so you never type it again. Leave the password empty if it's in your `~/.pgpass` file.
   - **Display** — an optional **Name** (defaults to the database name) and a **Dump prefix** (see below).
   - **One-click refresh** — where this database's newest dump lives. Click **Use current S3 source** to copy what's selected on the Download tab, or fill in **AWS profile**, **Bucket**, **Folder** and optionally **Name contains**.
3. Click **Test connection**. You should see *✓ Connected in … ms*.
4. Click **Add database**.

Repeat for every database you refresh (for example one per project, or staging and production copies).

### What is a "Dump prefix"?

If your dump files are named like `gen_2026-11-20.dump` and `mad_2026-11-20.dump`, the part before the `_` tells which database they belong to. Give each database its prefix (`gen`, `mad`) and DB Refresh picks the right database automatically whenever you choose such a file.

Your dumps don't have a prefix? You can add one when downloading (see *Quick names* below).

---

## 4. Refresh a database in one click

On the **⇄ Sync database** tab, every database with a one-click source has a **Refresh latest** button.

1. Click **Refresh latest**.
2. DB Refresh finds the newest dump and shows what it will do:
   - the dump's name, size and age,
   - whether it must be downloaded or is **already on disk**,
   - which database it will replace.
3. Click **Refresh now**.

Progress appears in the **Restore** panel: **Download → Check dump → Connect → Restore data → Done**. You can switch tabs while it runs.

No **Refresh latest** button? Click **Set up one-click refresh →** on the database card and fill in the *One-click refresh* section.

---

## 5. Find and download dumps (Download tab)

### Search

1. In **S3 source**, pick your **AWS profile**.
2. Type the **Bucket** name and press Enter. Bucket names are always lowercase; the app corrects capitals for you.
3. Click the **Folder** button to browse into the bucket's folders. Click a folder to go inside, then **Use "…"** to select it. Breadcrumbs at the top take you back up.
4. Optionally type **Name contains** (e.g. `gen_`) to filter.
5. Click **Search dumps** (or press Enter in *Name contains*).

Your bucket, folder and profile are remembered for next time.

### The Backups list

- The newest **10 dumps** are shown first; use **Show 10 more** or **Show all** for older ones.
- **Latest** marks the newest dump of each kind. Coloured tags (e.g. **GEN**, **MAD**) show the dump prefix.
- Hover the age (e.g. *6h ago*) to see the exact time.
- **✓ On disk** means the file is already downloaded.

### Download (and rename)

Click **↓ Download** on a dump. A dialog opens:

- **Save as** — change the file name if you like. `.dump` is added if you leave it out.
- **Quick names** — one click adds a database's prefix, e.g. `2026-11-20.dump` → `gen_2026-11-20.dump`, so the Sync tab recognises it.
- A warning appears if a file with that name already exists.

Click **Download**. The **Downloads** panel at the bottom shows progress, speed and time left. You can **Cancel**, **Retry**, **Show in Finder**, or go straight to restoring with **⇄ Sync to database…**.

Files are saved in `~/Downloads/db-refresh-dumps/`.

---

## 6. Restore a specific dump (Sync tab)

Use this when you want an older dump, or one from somewhere else on your Mac.

1. Under **1 · Dump file**, choose a downloaded dump — or **Choose another file…** for any `.dump` file on your Mac.
2. Under **2 · Database**, choose where it goes (picked automatically if the file has a matching prefix).
3. Click **Sync now**, read the warning, and click **Replace data**.

Shortcut: **⇄ Sync…** on a downloaded dump in the Backups list opens this screen with the file already selected.

> **Important:** restoring **replaces** the database. Existing tables are dropped and recreated (`pg_restore --clean`). Anything you changed in that database is lost.

### Reading the result

| Message | What it means |
|---|---|
| ✓ *database is now a copy of …* | Everything was restored. |
| ⚠ *restored, but N item(s) failed and were skipped* | The data is in, but some items failed — often harmless (e.g. extensions or roles that don't exist locally). Click **Errors** in the log to see them. |
| ✗ *Sync failed* | The restore stopped. Click **Errors** in the log for the reason. |
| *Cancelled* | You pressed Cancel. The database may be half-restored — run the sync again. |

### The log

- **All / Errors / Warnings** filter the log lines.
- **Copy log** copies the whole log, handy for asking a colleague for help.
- Errors are red, warnings yellow, success green.

---

## 7. Your data and privacy

| What | Where |
|---|---|
| Bucket, folder, databases (no passwords) | `~/Library/Application Support/db-refresh/settings.json` |
| Database passwords | macOS **Keychain** (encrypted), under `dev.dbrefresh.app` |
| Downloaded dumps | `~/Downloads/db-refresh-dumps/` |

Nothing is sent anywhere except to your S3 bucket (to list and download) and your database (to restore). Passwords are only given to the PostgreSQL tools while they run.

---

## 8. Troubleshooting

**"Could not get AWS credentials for profile …"**
Your AWS login is missing or expired. For SSO, run `aws sso login --profile <name>` in Terminal, then search again.

**"Access denied" / "Bucket … does not exist" / "different region"**
Check the bucket name, and that your profile has access to it. For region errors, set the right `region` for that profile in `~/.aws/config`.

**"psql is not installed"**
Run `brew install libpq` in Terminal.

**"Could not connect: … password authentication failed"**
Edit the database (✎ on its card), type the password again, keep **Remember password in macOS Keychain** on, and **Save changes**.

**macOS asks to allow Keychain access**
Click **Always Allow**. It may ask again after installing a new version of the app.

**"DB Refresh can't be opened" / "is damaged"**
See *Install* above: **Open Anyway** in Privacy & Security, or the `xattr` command.

**The restore finished "with N errors"**
Click **Errors** in the log. Messages about extensions, owners or roles are usually safe to ignore. If tables you need are missing, share the copied log with your team.

---

## 9. Questions

**Can I use it for several projects?**
Yes. Add one database per project, each with its own one-click source (bucket and folder).

**How do I free up disk space?**
Delete old files in `~/Downloads/db-refresh-dumps/` (use **Show in Finder**).

**How do I uninstall?**
Drag **DB Refresh** from Applications to the Trash. To remove its data too, delete `~/Library/Application Support/db-refresh/`, `~/Downloads/db-refresh-dumps/`, and the `dev.dbrefresh.app` items in Keychain Access.
