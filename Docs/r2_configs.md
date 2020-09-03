# Configuration file

The configuration file is by default assumed to be /etc/r2.cfg if it exists, or else it can be suppled as a command line parameter 'r2 -c <config file>' .. The config file is in the INI format, same as whats used by the cargo.toml files. It has different sections explained below

# general

```
[general]
pkts=4096
particles=8192
particle_sz=2048
threads=4
```

This means that r2 should run with with a pool of 4096 packets and 8192 particles of size 2048, and total four data forwarding threads. Each of these has its defaults, so any of them can be safely omitted.

# dpdk

See dpdk/dpdk.md to see the dpdk configuration options