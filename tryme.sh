#!/usr/bin/env bash

# If R2 is already running, kill it, this script will again launch R2 
pkill r2

# Create two docker containers using the tiny busybox image 
docker create -t --name R2_client1 busybox sh
docker create -t --name R2_client2 busybox sh
docker start R2_client1
docker start R2_client2

# Create veth interface pairs
ip link add veth_r2_1 type veth peer name veth_c2_1
ip link add veth_r2_2 type veth peer name veth_c2_2

# Get pids of the docker namespace
c1_pid=`docker inspect --format '{{ .State.Pid }}' R2_client1`
c2_pid=`docker inspect --format '{{ .State.Pid }}' R2_client2`
# Move the c2 end of veths to the dockers namespace
ip link set netns $c1_pid dev veth_c2_1
ip link set netns $c2_pid dev veth_c2_2
# Set the links to up state 
nsenter -t $c1_pid -n ip link set veth_c2_1 up
nsenter -t $c2_pid -n ip link set veth_c2_2 up
# Configure ip addresses on the docker end
nsenter -t $c1_pid -n ip addr add 1.1.1.1/24 dev veth_c2_1
nsenter -t $c2_pid -n ip addr add 2.1.1.1/24 dev veth_c2_2
# Delete default routes on both containers
nsenter -t $c1_pid -n ip route del default
nsenter -t $c2_pid -n ip route del default
# Point default route to our new interfaces
nsenter -t $c1_pid -n ip route add default via 1.1.1.2 dev veth_c2_1
nsenter -t $c2_pid -n ip route add default via 2.1.1.2 dev veth_c2_2

# compile R2
~/.cargo/bin/cargo build

# Run R2
./target/debug/r2 &

# Sometimes the interfaces take a while to come up, so wait for couple
# of seconds and bring the interfaces up
sleep 2
ip link set veth_r2_1 up
ip link set veth_r2_2 up

# Add one end of the veth pairs to R2, with some random mac address
./target/debug/r2intf veth_r2_1 add 0 8a:61:da:68:46:76
./target/debug/r2intf veth_r2_2 add 1 0e:67:57:1b:68:9c

# Add ip addresses in the corresponding subnets that we added to the docker
./target/debug/r2intf veth_r2_1 ip 1.1.1.2/24
./target/debug/r2intf veth_r2_2 ip 2.1.1.2/24

