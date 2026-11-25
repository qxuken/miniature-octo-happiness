# Study authenticated receive interruption

The test contacts a running receiver with the configured peer
credentials. The fixture waits until the authenticated session reaches
the application adapter, then closes the client without an offer.
The receiver reports one failure and zero successful sessions. In JSON
mode the nested diagnostic retains the replication phase and engine
`Io` code. In text mode it retains the original `Io` error.

| Observation | State |
| --- | --- |
| The authenticated client disconnects before the offer | The receiver stays at revision 2 with zero minutes and identical file bytes. |
| The sender commits seven minutes locally | It observes its own revision 3 before shipping. |
| The sender retries over a fresh authenticated session | The existing receiver advances to revision 3 and acknowledges seven minutes with its configured actor and policy. |
| The receiver stops and the database reopens | Revision 3 retains seven minutes. |

The two fault controls substitute the error returned by the receiving
wrapper or change the structured phase. The fixture checks both
diagnostic modes and the normal completion of the same wrapper.
