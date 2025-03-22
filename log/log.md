---
title: "LOG"
weight: 1
type: docs
description: >

---

# Forwarding plane logger

So the first thing to be clarified upfront is that this is NOT a general purpose syslog kind of logger. This is purely meant for forwarding path to log per-packet information, and not expected to be on by default, usually used for a limited time to collect data to debug some issue. So obviously the logger has to be a simple fast module. These are the properties of the logger

1. The logger is per thread - there is no locks or atomic operatons etc.. when a thread logs to its logger. Its just a straightforward write to memory. So obviously an external utility that displays the logs will have to "merge" the logs from each thread according to timestamps before its displayed to the user

2. The logger memory is in shared memory. So an external utility can dump the logger without disturbing the R2 process, although thats not how its done today. Today the external utility makes an API call to R2 to dump the logs, the API callback sets a flag asking forwarding paths to stop using their loggers, and waits till it knows the forwarding paths have stopped using the logger, and then the API callback handler dumps the logs as serialized json - note that it doesnt merge the logs, thats upto the external utility. We can (and I think we should) offload R2 from serializing logs, R2 should just make the forwarding paths stop using the logger and then the external utility can/should do everything else. But this will need the data in Logger::hash to also be available to the external utility, to be able to interpret the log entries.

3. The shared memory area for logs is divided into fixed size chunks of memory - so its basically a circular list of fixed size objects. And hence obviously each log entry has a fixed size and hence theres a max limit to what can be dumped in each log entry - all these choices are to keep the logger simple and fast.

## The logger macro

Modules dump a log entry using a log! macro. The macro takes variable number of parameters and uses a log_helper! macro to recursively walk through each parameter and copy it to the log entry. The macro just determines the size of each parameter, treats it as an array of bytes and copies those bytes into a log entry. So logging an entry is a bunch of mem copies.

As described in the architecture section, The logger is shared between the API handler (control) thread and the forwarding plane thread, so that the API handler can dump the logs. Because of this sharing, the logger automatically becomes read-only in Rust. And obviously we want to modify the logger in the forwarding plane - for example to get a new log entry, ie advance the log entry head/tail etc.. So for that purpose we use the "interior mutability" concept in Rust - we basically use the indices as Atomic numbers. But that does not really introduce an atomic operation because we use the Relaxed memory mode - remember this atomic is just to get around Rust making the shared logger read-only. And the relaxed mode in Intel CPUs just translates to a regular memory read/write, no atomics.

## Serialization

To dump logger entries as json, I initially considered the serdes module in Rust. But as described in the architecture, we want to do everything possible to keep R2 small and its dependencies minimal. The serdes module is a rather large module and R2 depending on it to just dump a simple json structure did not make sense - so the logger module just dumps the entry as hand-coded json. Its ugly, but it does avoid huge unnecessary dependency, and it should be fine as long as the data being dumped remains flat and straight forward (which will be the case for a log entry)

