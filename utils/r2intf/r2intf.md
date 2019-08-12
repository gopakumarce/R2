# Usage

This utility is used for adding a new interface and configuring the interface parameters like ip address and QoS queues. Example usages of r2intf are below

## Add an interface

Parameters are interface name, ifindex and mac address

./target/debug/r2intf eth0 add 0 8a:61:da:68:46:76

## Add an IP address

Format is ipaddress/mask

./target/debug/r2intf eth0 ip 1.1.1.2/24

## Adding QoS classes

Right now the scheduler supported is HFSC. You will have to get familiar with HFSC concepts of realtime (r), fair share (f) and upper limit (u) - and each of those varieties has a curve with parameters m1, m2, and d. So we configure a QoS class on the interface specifying a class name and a parent name and the parameters of interest above. The interface by default has a class called with name 'root', so the first class added will have a parent of name 'root'