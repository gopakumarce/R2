# Scheduler

The only scheduler we support today is HFSC, and the code is completely modelled after the BSD version of HFSC (the linux version is quite similar to the BSD version too). Really the only authoritative "documentation" on HFSC is <https://www.cs.cmu.edu/~hzhang/papers/SIGCOM97.pd> - thats a terse reading, so good luck with that :P

There needs to be a bit more layers of abstraction here as and when more schedulers are added, right now the IfNode is hardcoded to assume its an HFSC scheduler. And the utils/r2intf also configures various HFSC curves, the configuration will be different if its a different scheduler.

As of today in our hfsc implementation, we do not support upper limit curves - only fair share and realtime are supported.
