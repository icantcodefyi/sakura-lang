# Reports

No model episodes have run. `ledger.json` is the committed spend ledger for every
live run (owner cap: $10 total); the runner refuses to run live without it, so never
delete or reset it. Commit it after every live run (the workflow does this itself). Do not interpret offline gold/hollow validation as a
model success rate. Future reviewed reports must include every episode, failed
and censored outcomes, raw counts, uncertainty intervals, total cost, exact model
and compiler/bundle revisions, and activation/retrieval failures. Raw provider
messages stay private until reviewed and redacted; never commit keys or secrets.
