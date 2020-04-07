# Trying R2 

The goal here is to provide a test setup as below, where R2_client1 and R2_client2 are two docker containers from which we can originate packets like ping and R2 in the middle, is the router.

```c
R2_client1 [veth_c2_1]----[veth_r2_1] R2 [veth_r2_2]----[veth_c2_2] R2_client2
             1.1.1.1        1.1.1.2        2.1.1.2        2.1.1.1
```

The steps below have been tested on brand new Ubuntu installations 18.04 and 16.04 server AND desktop. So for other versions of ubuntu or other distributions of linux, or if you have an already running ubuntu you have mucked around with, there might have to be some modifications to the steps below. 14.04 ubuntu has a different set of steps to install docker, so I did not list that here, but if you have 14.04, just get docker and docker CLIs installed (step 2) and rest of the steps are the same. Also you might have some packages already like git, gcc etc.. in which case those apt-gets are just ignored

## Four steps

1. Install rust as mentioned here - <https://www.rust-lang.org/tools/install>

   ```c
   sudo apt install curl
   sudo apt install gcc
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install docker as below. Docker installation steps below should go through fine. But in case you face issues, more docker information is here <https://docs.docker.com/install/linux/docker-ce/ubuntu/>.

   ```c
   sudo apt-get install -y apt-transport-https ca-certificates curl gnupg-agent software-properties-common
   curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
   sudo add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable"
   sudo apt-get update
   sudo apt-get install -y docker-ce docker-ce-cli containerd.io
   sudo docker pull busybox
   ```

3. Download the source code from here - <https://github.com/gopakumarce/R2>
  
   ```c
   sudo apt install git
   git clone git@github.com:gopakumarce/R2.git
   ```

4. In the R2 source code root directory, type the below command. The first two commands 'sudo usermod' and 'newgrp docker' adds your user to the docker group so you can create docker containers etc.. with your userid. The first time compilation can take a few seconds, the R2 build system 'cargo', downloads source code for all dependencies and compiles them them the first time. NOTE: Various things inside the script like creating new interfaces etc.., are also run with 'sudo' permissions

   ```c
   sudo usermod -aG docker $USER
   newgrp docker
   cd R2
   ./tryme.sh
   ```

## Play around, have fun

Once step4 is complete, attach to the containers, type route -n, ifconfig etc.. to see the interfaces and ip addresses, and ping from one container to the other. The ping gets routed via R2. Use commands below to attach to either container, ctrl-d to exit. Commands to attach to each container and ping the ip address in the other container, is below. R2 itself does not respond to ping today, so if you ping R2 itself, that will fail.

```c
docker exec -it R2_client1 sh
ping 2.1.1.1
docker exec -it R2_client2 sh
ping 1.1.1.1
```

You can play around further for example by adding more loopback interfaces inside the containers, assign it ip addresses like 3.1.1.1, 4.1.1.1 etc.. and add routes in R2 to point the route to the right container interface NOTE: So why dint we have to add routes for the simple setup above ? Its because we were just pinging the connected subnets of each interface. And R2 by default inserts a connected/network route for its interfaces

```c
sudo ./target/debug/r2rt route 3.1.1.1/32 2.1.1.1 veth_r2_2
sudo ./target/debug/r2rt route 4.1.1.1/32 1.1.1.1 veth_r2_1
```

## Contributing to R2 

To contribute to R2, pull R2, make bug fixes, run "cargo test" and ensure everything passes and send a merge request. If you are adding new functionalities, there should be unit test cases which can be run by cargo test.
