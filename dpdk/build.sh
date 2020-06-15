#/usr/bin/env bash

cd $1;
curl -o dpdk.tar.xz https://fast.dpdk.org/rel/dpdk-19.11.2.tar.xz
tar xJf dpdk.tar.xz
cd dpdk-stable-19.11.2
meson build
cd build; ninja

