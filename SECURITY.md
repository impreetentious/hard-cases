# Security policy

Hard Cases is an educational optimization laboratory, not production
infrastructure. It exposes no network surface, no authentication, and no
sandbox, and it makes no warranty about the results it produces.

Two surfaces are worth reporting against.

**Parsers.** The solver and the verifier each read untrusted files — MPS, the
LP-format subset, JSON models, and certificates — with hand-rolled parsers.
Both are expected to reject malformed input with a structured error inside
documented allocation, exponent, and depth caps. A panic, a hang, a stack
overflow on a deeply nested proof, or an allocation driven by an attacker-
supplied count is a bug worth reporting even when the input is obvious
garbage. The WebAssembly build parses whatever a page hands it.

**The verifier's verdict.** A certificate that the verifier accepts but that
does not actually prove its claim is the most serious defect this project can
have, ahead of any crash. If you can construct one, that is a security report
in every sense that matters here: send the original model bytes, the
certificate, and the claim it makes.

Please do not disclose a suspected vulnerability in a public issue before I
have had a chance to assess it. Email sps.daemon@gmail.com, or use GitHub
private vulnerability reporting / a GitLab confidential issue if this
repository is hosted there. Include the affected build, the exact command, and
the smallest reproducing input. Models and seeds may be shared publicly after
triage.
