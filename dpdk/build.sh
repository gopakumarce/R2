#/usr/bin/env bash

DPDK_VERSION=dpdk-19.11.2.tar.xz

cd $1
mkdir dpdk; cd dpdk;
curl -o dpdk.tar.xz https://fast.dpdk.org/rel/$DPDK_VERSION
tar -xJf dpdk.tar.xz --strip=1
meson build
cd build; ninja

