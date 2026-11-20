# DB Refresh - User Guide

DB Refresh keeps a local PostgreSQL database up to date with your backups. It finds database dumps in an S3 bucket, downloads them to your Mac, and restores them into a local database - for any project, as often as you like.

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
   - **Quick fill** - paste a connection URL such as `postgres://app_user:secret@localhost:5432/app_db` and the fields below fill in for you. (The URL box clears itself because it contains your password.)
   - **Connection** - Host, Port, Database and User. The line under the fields shows where data will go, e.g. `app_user@localhost:5432/app_db`.
   - **Authentication** - your database password. Turn on **Remember password in macOS Keychain** so you never type it again. Leave the password empty if it's in your `~/.pgpass` file.
     With a remembered password, **Ask for Touch ID before using it** (on by default) asks for your fingerprint each time the password is used - or your Mac password if Touch ID isn't available.
   - **Display** - an optional **Name** (defaults to the database name) and a **Dump prefix** (see below).
   - **Before restoring** - what happens to the data already there:
     - **Drop the dump's schemas** (default) - every schema in the dump (e.g. `auth`, `core`) is dropped with CASCADE, then restored. You get an exact copy of those schemas; other schemas are kept. Your database user must own those schemas. If it can't drop one, nothing is changed.
     - **Recreate the database** - the whole database is deleted and created again. Your user must own the database and be allowed to create databases.
     - **Replace objects** - only `pg_restore --clean`. Leftover local objects can cause errors and a mix of old and new data, so use it only if the other two aren't possible.
   - **One-click refresh** - where this database's newest dump lives. Click **Use current S3 source** to copy what's selected on the Download tab, or fill in **AWS profile**, **Bucket**, **Folder** and optionally **Name contains**.
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

- **Save as** - change the file name if you like. `.dump` is added if you leave it out.
- **Quick names** - one click adds a database's prefix, e.g. `2026-11-20.dump` → `gen_2026-11-20.dump`, so the Sync tab recognises it.
- A warning appears if a file with that name already exists.

Click **Download**. The **Downloads** panel at the bottom shows progress, speed and time left. You can **Cancel**, **Retry**, **Show in Finder**, or go straight to restoring with **⇄ Sync to database…**.

Files are saved in your **Downloads** folder by default. To use another folder, click **Change…** next to the folder in the **Downloads** panel or in the download dialog; **Use Downloads** switches back.

---

## 6. Restore a specific dump (Sync tab)

Use this when you want an older dump, or one from somewhere else on your Mac.

1. Under **1 · Dump file**, choose a downloaded dump - or **Choose another file…** for any `.dump` file on your Mac.
2. Under **2 · Database**, choose where it goes (picked automatically if the file has a matching prefix).
3. Click **Sync now**, read the warning, and click **Replace data**.

Shortcut: **⇄ Sync…** on a downloaded dump in the Backups list opens this screen with the file already selected.

> **Important:** restoring **replaces** data. Depending on the database's **Before restoring** setting, the dump's schemas are dropped, the whole database is recreated, or each object is replaced. Anything you changed there is lost.

### Reading the result

| Message | What it means |
|---|---|
| ✓ *database is now a copy of …* | Everything was restored. |
| ⚠ *restored, but N item(s) failed and were skipped* | The data is in, but some items failed - often harmless (e.g. extensions or roles that don't exist locally). Click **Errors** in the log to see them. |
| ✗ *Sync failed* | The restore stopped. Click **Errors** in the log for the reason. |
| *Cancelled* | You pressed Cancel. The database may be half-restored - run the sync again. |

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
| Downloaded dumps | `~/Downloads` by default, or the folder you chose with **Change…** |

Nothing is sent anywhere except to your S3 bucket (to list and download) and your database (to restore). Passwords are only given to the PostgreSQL tools while they run.

---

## 8. Troubleshooting

**"Could not get AWS credentials for profile …"**
Your AWS login is missing or expired. For SSO, run `aws sso login --profile <name>` in Terminal, then search again.

**"Access denied" / "Bucket … does not exist" / "different region"**
Check the bucket name, and that your profile has access to it. For region errors, set the right `region` for that profile in `~/.aws/config`.

**"… was replaced in S3 while downloading"**
A new backup was uploaded under the same name during your download, so the app stopped rather than mix two versions. Download it again.

**Downloads are slow**
DB Refresh fetches 8 parts of a dump at once. If it's still slow, the limit is usually your network or VPN - try without the VPN if your setup allows it.

**"psql is not installed"**
Run `brew install libpq` in Terminal.

**"Could not connect: … password authentication failed"**
Edit the database (✎ on its card), type the password again, keep **Remember password in macOS Keychain** on, and **Save changes**.

**macOS asks for your Mac password to allow Keychain access**
Click **Always Allow**. Builds signed with the app's signing certificate keep that permission across updates, so this should be a one-time question. (The Touch ID prompt is different: it appears on purpose when **Ask for Touch ID before using it** is on.)

**"Touch ID was cancelled, so the saved password was not used"**
You dismissed the fingerprint prompt. Sync again and confirm with Touch ID, or type the password in the dialog instead.

**"DB Refresh can't be opened" / "is damaged"**
See *Install* above: **Open Anyway** in Privacy & Security, or the `xattr` command.

**"cannot drop schema … because other objects depend on it" and "schema … already exists"**
Your local database contains objects that are not in the dump (for example tables from local work), and the database is set to **Replace objects**. Leftovers can block the restore and leave old and new rows mixed. Edit the database, choose **Drop the dump's schemas** under **Before restoring**, and sync again.

**"Could not drop the schemas, so nothing was changed"**
**Drop the dump's schemas** needs your database user to own those schemas. Ask your database admin to run `ALTER SCHEMA <name> OWNER TO <your user>;` for each, or choose another mode. Nothing was dropped.

**"… may not create databases, so … was left untouched"**
**Recreate the database** needs permission to create databases. Ask your database admin to run `ALTER ROLE <your user> CREATEDB;` - or choose **Drop the dump's schemas**. Nothing was deleted.

**The restore finished "with N errors"**
Click **Errors** in the log. Messages about extensions, owners or roles are usually safe to ignore. If tables you need are missing, share the copied log with your team.

---

## 9. Questions

**Can I use it for several projects?**
Yes. Add one database per project, each with its own one-click source (bucket and folder).

**How do I free up disk space?**
Delete old `.dump` files from your download folder (use **Show in Finder**).

**How do I uninstall?**
Drag **DB Refresh** from Applications to the Trash. To remove its data too, delete `~/Library/Application Support/db-refresh/`, the downloaded `.dump` files, and the `dev.dbrefresh.app` items in Keychain Access.
