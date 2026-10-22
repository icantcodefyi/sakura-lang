# Query SQLite with bound parameters

Keep SQL text literal and interpolate values as parameters, then decode typed rows.

`find_notes` returns `List<Note>`, which determines the row schema for `Sql.query`.
Each `sql"…"` interpolation becomes a bound parameter; neither string concatenation
nor a trust bypass is needed. `SqlError` propagates from the database operations.

Tests install `Sql.memory`, a real fresh in-memory SQLite database, so no filesystem
permission or persistent database is needed. One test proves SQL-looking input
matches only its literal value. Another runs a transaction that writes and then
fails on an absent table, checks `SqlError`, and proves the first write rolled back.
Only the tests run. For a persistent database, query `sakura doc std::sql::sqlite`
and supply an appropriately confined filesystem handler and manifest permissions.
