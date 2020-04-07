#!/usr/bin/env bash

# Exit on error
set -e

# Remove containers
rm_containers () {
    # if containers already exist, get their container ID
    C1=$(docker ps -a | grep R2_client1 | awk '{print $1}')
    C2=$(docker ps -a | grep R2_client2 | awk '{print $1}')

    if [[ ! -z $C1 ]]; then
        docker stop $C1 || true
        docker rm $C1 || true
    fi
    if [[ ! -z $C2 ]]; then
        docker stop $C2 || true
        docker rm $C2 || true
    fi
}

rm_containers

# Do not run this script as root, we use sudo for priv cmds
if [[ $EUID == 0 ]]; then
  echo >&2 "error: Do not run this script as root"
  exit 1
fi

# If R2 is already running, kill it, this script will again launch R2 
sudo pkill r2 || true

# Create two docker containers using the tiny busybox image 
docker create -t --name R2_client1 busybox sh 
docker create -t --name R2_client2 busybox sh
docker start R2_client1 
docker start R2_client2

# Get the container IDs for cleanup
C1=`docker ps | grep R2_client1 | awk '{print $1}'`
C2=`docker ps | grep R2_client2 | awk '{print $1}'`

# Create veth interface pairs
sudo ip link add veth_r2_1 type veth peer name veth_c2_1
sudo ip link add veth_r2_2 type veth peer name veth_c2_2

# Get pids of the docker namespace
c1_pid=`docker inspect --format '{{ .State.Pid }}' R2_client1`
c2_pid=`docker inspect --format '{{ .State.Pid }}' R2_client2`
# Move the c2 end of veths to the dockers namespace
sudo ip link set netns $c1_pid dev veth_c2_1
sudo ip link set netns $c2_pid dev veth_c2_2
# Set the links to up state 
sudo nsenter -t $c1_pid -n ip link set veth_c2_1 up
sudo nsenter -t $c2_pid -n ip link set veth_c2_2 up
# Configure ip addresses on the docker end
sudo nsenter -t $c1_pid -n ip addr add 1.1.1.1/24 dev veth_c2_1
sudo nsenter -t $c2_pid -n ip addr add 2.1.1.1/24 dev veth_c2_2
# Delete default routes on both containers
sudo nsenter -t $c1_pid -n ip route del default
sudo nsenter -t $c2_pid -n ip route del default
# Point default route to our new interfaces
sudo nsenter -t $c1_pid -n ip route add default via 1.1.1.2 dev veth_c2_1
sudo nsenter -t $c2_pid -n ip route add default via 2.1.1.2 dev veth_c2_2

# compile R2
~/.cargo/bin/cargo build

# Run R2
sudo ./target/debug/r2 &

# Sometimes the interfaces take a while to come up, so wait for couple
# of seconds and bring the interfaces up
sleep 2
sudo ip link set veth_r2_1 up
sudo ip link set veth_r2_2 up

# Add one end of the veth pairs to R2, with some random mac address
./target/debug/r2intf veth_r2_1 add 0 8a:61:da:68:46:76
./target/debug/r2intf veth_r2_2 add 1 0e:67:57:1b:68:9c

# Add ip addresses in the corresponding subnets that we added to the docker
./target/debug/r2intf veth_r2_1 ip 1.1.1.2/24
./target/debug/r2intf veth_r2_2 ip 2.1.1.2/24

