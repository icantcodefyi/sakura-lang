# Offline validation boundary

`offline.py` validates task metadata and executes trusted, checked-in gold and
negative fixtures with the locally built Sakura CLI. It is invoked by the Cargo
integration test `agent_dx`; it needs Python 3.11+ and Git, with no third-party
Python packages, secrets or model calls.

This is **not a sandbox or model runner**. Do not point it at untrusted model
output on the host. Opus's runner must supply process/filesystem/network isolation,
provider accounting, approval, transcript replay, report generation and immutable
external baselines before paid evaluation. Oracle sources, gold and hollow trees
must never be copied into the agent's workspace. The offline checks materialize
oracles only in a separate verifier project after the candidate is frozen.
