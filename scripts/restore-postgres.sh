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
echo "=========================================="

# ==========================================
# Test Connection
# ==========================================

echo "Testing connection..."

psql -X -w \
    -h "$HOST" \
    -p "$PORT" \
    -U "$USER" \
    -d "$DATABASE" \
    -v ON_ERROR_STOP=1 \
    -c "SELECT current_database();" || {
        echo "ERROR: Database connection failed (check ~/.pgpass or PGPASSWORD)."
        exit 1
    }

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
    --clean --if-exists \
    "$DUMP_FILE"

ELAPSED=$(( $(date +%s) - START_TIME ))

echo "=========================================="
echo " RESTORE COMPLETED SUCCESSFULLY"
echo " Time taken: $ELAPSED seconds"
echo "=========================================="
