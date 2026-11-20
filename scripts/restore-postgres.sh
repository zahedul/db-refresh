#!/bin/bash
#
# Restore a PostgreSQL custom-format dump into a database.
#
# Usage:
#   restore-postgres.sh <dump_file> <database>
#
# Connection settings come from the environment (with local defaults):
#   PGHOST (localhost)  PGPORT (5432)  PGUSER (postgres)  RESTORE_JOBS (4)
#
# Clean slate (optional):
#   RECREATE_DATABASE=1   drop and create the database before restoring, so it becomes an
#                         exact copy of the dump. Other connections to it are closed first.
#                         The user must own the database and have CREATEDB.
#   MAINTENANCE_DB        database to connect to while recreating (postgres)
#
# Password: never stored here. Postgres tools read it from ~/.pgpass
# (line format: host:port:database:user:password, file mode 600),
# or from PGPASSWORD if the caller sets it.

set -Eeuo pipefail

export PATH="/opt/homebrew/opt/libpq/bin:$PATH"

if [[ $# -ne 2 ]]; then
    echo "Usage: $(basename "$0") <dump_file> <database>"
    exit 2
fi

DUMP_FILE="$1"
DATABASE="$2"
HOST="${PGHOST:-localhost}"
PORT="${PGPORT:-5432}"
USER="${PGUSER:-postgres}"
JOBS="${RESTORE_JOBS:-4}"
RECREATE_DATABASE="${RECREATE_DATABASE:-0}"
MAINTENANCE_DB="${MAINTENANCE_DB:-postgres}"

export PGCONNECT_TIMEOUT=10

# ==========================================
# Validation
# ==========================================

for cmd in pg_restore psql; do
    command -v "$cmd" >/dev/null 2>&1 || {
        echo "ERROR: $cmd is not installed."
        exit 1
    }
done

if [[ ! -f "$DUMP_FILE" ]]; then
    echo "ERROR: Dump file not found: $DUMP_FILE"
    exit 1
fi

pg_restore -l "$DUMP_FILE" >/dev/null || {
    echo "ERROR: Invalid PostgreSQL dump."
    exit 1
}

echo "=========================================="
echo " PostgreSQL Restore"
echo "=========================================="
echo "Host       : $HOST"
echo "Port       : $PORT"
echo "User       : $USER"
echo "Database   : $DATABASE"
echo "Dump       : $DUMP_FILE"
echo "Jobs       : $JOBS"
echo "Recreate   : $( [[ "$RECREATE_DATABASE" == 1 ]] && echo yes || echo no )"
echo "=========================================="

# ==========================================
# Test Connection
# ==========================================

echo "Testing connection..."

# When recreating, the target may not exist yet, so test the maintenance database instead.
CONNECT_DB="$DATABASE"
[[ "$RECREATE_DATABASE" == 1 ]] && CONNECT_DB="$MAINTENANCE_DB"

psql -X -w \
    -h "$HOST" \
    -p "$PORT" \
    -U "$USER" \
    -d "$CONNECT_DB" \
    -v ON_ERROR_STOP=1 \
    -c "SELECT current_database();" || {
        echo "ERROR: Database connection failed (check ~/.pgpass or PGPASSWORD)."
        exit 1
    }

# ==========================================
# Recreate Database (clean slate)
# ==========================================

CLEAN_OPTIONS=(--clean --if-exists)

if [[ "$RECREATE_DATABASE" == 1 ]]; then
    echo "Recreating database $DATABASE..."

    # Check CREATEDB before dropping: otherwise an owner without it would lose the
    # database (DROP succeeds, CREATE fails). DROP itself enforces ownership.
    CAN_CREATE_DATABASES="$(psql -X -w -tA \
        -h "$HOST" \
        -p "$PORT" \
        -U "$USER" \
        -d "$MAINTENANCE_DB" \
        -c "SELECT rolsuper OR rolcreatedb FROM pg_roles WHERE rolname = current_user;")"
    if [[ "$CAN_CREATE_DATABASES" != "t" ]]; then
        echo "ERROR: $USER may not create databases, so $DATABASE was left untouched. Allow it with: ALTER ROLE $USER CREATEDB;"
        exit 1
    fi

    # Variables (:'db' / :"db") make psql quote the name safely. Statements run one by
    # one because DROP/CREATE DATABASE cannot run inside a transaction.
    psql -X -w -q \
        -h "$HOST" \
        -p "$PORT" \
        -U "$USER" \
        -d "$MAINTENANCE_DB" \
        -v ON_ERROR_STOP=1 \
        -v db="$DATABASE" >/dev/null <<'SQL' || {
SELECT pg_terminate_backend(pid)
  FROM pg_stat_activity
 WHERE datname = :'db' AND pid <> pg_backend_pid();
DROP DATABASE IF EXISTS :"db";
CREATE DATABASE :"db";
SQL
        echo "ERROR: Could not recreate $DATABASE. The user must own it and be allowed to create databases (ALTER ROLE $USER CREATEDB;)."
        exit 1
    }

    echo "Database $DATABASE recreated (empty)."
    # The database is empty, so there is nothing to clean.
    CLEAN_OPTIONS=()
fi

# ==========================================
# Restore Database
# ==========================================

echo "Starting database restoration..."

START_TIME=$(date +%s)

# -w: never prompt for a password; the app has no terminal to type into.
pg_restore -w \
    -h "$HOST" \
    -p "$PORT" \
    -U "$USER" \
    -d "$DATABASE" \
    -j "$JOBS" \
    --verbose \
    --no-owner \
    --no-acl \
    ${CLEAN_OPTIONS[@]+"${CLEAN_OPTIONS[@]}"} \
    "$DUMP_FILE"

ELAPSED=$(( $(date +%s) - START_TIME ))

echo "=========================================="
echo " RESTORE COMPLETED SUCCESSFULLY"
echo " Time taken: $ELAPSED seconds"
echo "=========================================="
