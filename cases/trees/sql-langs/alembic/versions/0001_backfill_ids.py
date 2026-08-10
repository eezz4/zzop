# MIGRATION TWIN of services/queries.py — the third half of the pair design, and the one whose two halves
# are BOTH expectations rather than one being silence.
#
# Admitting `.py` to three CRITICAL rules is what made this file necessary. Measured 2026-08-10 on the
# dogfood corpus (corpus/oss, 277 Python files / 16652 lines): the ONLY lines in the whole Python corpus
# matching any of the five widened patterns were five Alembic backfills in
# `be-fastapi-fs/backend/app/alembic/versions/`, and three of them survived the WHERE veto. Alembic is
# Python's dominant migration tool and its scripts do not live under `migrations/` — the default script
# location is `alembic/versions/`, which the shared migration vocabulary did not know. Without that arm,
# widening would have shipped three CRITICAL false positives on deliberate one-time backfills.
#
# So this file asserts both directions at once, which is what makes it evidence rather than a control:
#   * `sql/delete-no-where`, `sql/update-no-where`, `sql/truncate-in-app-code` must be SILENT here (the
#     path is a migration), and
#   * `sql/destructive-migration` must FIRE on each of the three, at info — the disclosure their messages
#     promise. A silent-only fixture would score green if the handoff rule had simply stopped admitting
#     `.py`, which is the exact reading the pairing design exists to remove.
revision = "0001"
down_revision = None


def upgrade():
    op.execute("UPDATE sqlx_py_mig_accounts SET migrated = 1")
    op.execute("DELETE FROM sqlx_py_mig_staging")
    op.execute("TRUNCATE TABLE sqlx_py_mig_scratch")
