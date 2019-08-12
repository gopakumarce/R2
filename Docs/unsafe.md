# Unsafe-Rust in R2

Anywhere we have to deal with raw pointers, we basically end up having to use unsafe Rust. Obviously the goal is to limit that to a small set of libraries which have no option but to do pointer manipulation. That current list is below. From what I know about packet forwarding systems, this would be all about it and I dont expect any more. So this means that we have to be extremely careful what we do in these libraries and we have to vet every pointer manipulation there and ensure they are bug free if we want to rely on Rust's memory safety for rest of the code.

The code in the unix/ directory deals with low level posix interactions with the system, and hence they are generally expected to be unsafe.

1. unix/shm - the shared memory library, here we deal with mmap() and getting virtual addresses etc.., so cant do without unsafe

2. unix/socket - the raw socket libray, to send and receive packets. And to send and receive packets we need write and read to/from the particle raw data, hence this also needs unsafe

3. unix/epoll - this calls some system calls via libc, like fcntl. This doesnt have to be unsafe. Its a TODO to replace this with a Rust library (does one exist ?) for epoll ?

4. counters: counters deal with taking shared memory addresses and converting it to Rust counter structure, so they end up being unsafe

5. log: logging is done by writing data to a log buffer, again ends up being unsafe

6. packet: The packet library deals with manipulating packet data in raw byte buffers, again ends up being unsafe. The default packet pool provided by the library just deals with buffers from the heap, but at some point we anticipate R2 to come up with say Intel dpdk based packet pools as an example - at which point the place where that packet pool is implemented in R2 will also have some unsafe semantics


I cannot re-emphasis the need to keep the amount of unsafe code to the absolute minimum. And like I mentioned before, having seen many packet forwarding systems, I anticipate that the above list is all there ever will be of unsafe code, and if we keep the above pieces of code small and simple and bug free, we can be assured that Rust will take care of the memory sanctity of R2