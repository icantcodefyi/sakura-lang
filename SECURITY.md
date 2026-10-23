# Security policy

## Supported versions

The current release is **`v1.0.0`**, the first stable release. Report vulnerabilities in it or
in `main`. A fix is made on `main` and ships in a new 1.x release; under
[STABILITY.md](STABILITY.md), a security fix that has to break behaviour comes with a migration
in the CHANGELOG. Earlier releases (the `1.0.0-beta.N` betas and the `0.x` previews) receive no
fixes, and no release line has a support window yet. Experimental features (the native
backend, packages, `std/worker`) are not a security boundary. This policy also covers this
repository's website and tooling.

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/icantcodefyi/sakuralang/security/advisories/new)
from the repository's **Security → Advisories → Report a vulnerability** page.
The initial security contact and triage owner is
[@icantcodefyi](https://github.com/icantcodefyi).

Never put exploit details, secrets, or personal data in a public issue, and do
not use ordinary issue forms for disclosures.

Include the affected revision/component, reproduction steps, impact, environment,
and any suggested fix or mitigation. Use minimal synthetic data; avoid testing
against other people's systems or accessing their data.

## Triage and coordinated disclosure

The maintainer acknowledges privately, reproduces the report, assesses impact,
and agrees on next steps with the reporter. This small volunteer project does
not promise a fixed response time or bounty; follow up in the same private thread
if you have not heard back. The maintainer owns remediation and advisory decisions.

Keep details private while a fix or mitigation is prepared. Agree on a disclosure
date and credit preferences with the reporter, validate the fix, and publish an
advisory with affected revisions, impact, remediation, and any remaining limits.
If exploitation is active, prioritize mitigation and communicate an accelerated
schedule. A disputed severity or timeline should be recorded privately and
revisited with the evidence.
