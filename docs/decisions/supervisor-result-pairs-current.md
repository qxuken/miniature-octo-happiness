# Supervisor result classification

| Classification | Required conditions |
| --- | --- |
| Ordinary success | Zero exit, no worker or cleanup failure and verified cleanup. |
| Expected error | A positive code, the exact exit status and diagnostic, verified cleanup and no cleanup failure, timeout, cancellation or explicit stop. |
| Controlled stop | Stop requested, SIGKILL exit, no worker or cleanup failure and verified cleanup. |

Each table starts with an accepted report. The test changes one condition
at a time and checks that classification rejects it. The nonpositive
code fixtures recompute the expected status and reason so those
comparisons still match. The source inventory recognizes each nested
short-circuit prefix. A condition can be false while the other conditions
in its prefix hold true. The predicates after a false condition are masked.

These cases exercise report classification after process supervision.
The process-level timeout, output limit and descendants tests retain
their original evidence scope.
